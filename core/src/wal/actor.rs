use super::*;

pub struct WalWriter {
    file: BufWriter<File>,
    wal_path: PathBuf,
    entry_count: u64,
    last_compact_time: Instant,
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

impl WalWriter {
    pub fn open(state_path: &str) -> Result<Self, String> {
        let wal_path = PathBuf::from(format!("{}/state.wal", state_path));

        // Ensure directory exists
        if let Some(parent) = wal_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create WAL directory: {}", e))?;
        }

        // Count existing valid entries (skip corrupt lines)
        let entry_count = if wal_path.exists() {
            let f = File::open(&wal_path)
                .map_err(|e| format!("Failed to open WAL for counting: {}", e))?;
            BufReader::new(f)
                .lines()
                .filter_map(|l| l.ok())
                .filter(|l| !l.trim().is_empty() && serde_json::from_str::<WalEntry>(l).is_ok())
                .count() as u64
        } else {
            0
        };

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal_path)
            .map_err(|e| format!("Failed to open WAL file: {}", e))?;

        Ok(Self {
            file: BufWriter::new(file),
            wal_path,
            entry_count,
            last_compact_time: Instant::now(),
        })
    }

    fn append_buffered(&mut self, entry: &WalEntry) -> Result<(), String> {
        let line = serde_json::to_string(entry)
            .map_err(|e| format!("Failed to serialize WAL entry: {}", e))?;
        self.file
            .write_all(line.as_bytes())
            .map_err(|e| format!("Failed to write WAL entry: {}", e))?;
        self.file
            .write_all(b"\n")
            .map_err(|e| format!("Failed to write WAL newline: {}", e))?;
        self.entry_count += 1;
        Ok(())
    }

    fn sync(&mut self) -> Result<(), String> {
        self.file
            .flush()
            .map_err(|e| format!("Failed to flush WAL: {}", e))?;
        self.file
            .get_ref()
            .sync_all()
            .map_err(|e| format!("Failed to fsync WAL: {}", e))
    }

    pub fn append(&mut self, entry: &WalEntry) -> Result<(), String> {
        self.append_buffered(entry)?;
        self.sync()
    }

    pub fn entry_count(&self) -> u64 {
        self.entry_count
    }

    /// Check if compact is needed based on entry count threshold or time interval.
    pub fn needs_compact(&self, threshold: u64) -> bool {
        if self.entry_count == 0 {
            return false;
        }
        self.entry_count >= threshold
            || self.last_compact_time.elapsed().as_secs() >= WAL_COMPACT_INTERVAL_SECS
    }

    /// Write a full snapshot to state.json and truncate the WAL.
    /// Accepts pre-serialized JSON to avoid borrow conflicts when wal and state
    /// are fields of the same struct.
    pub fn compact(&mut self, state_json: &str) -> Result<(), String> {
        let state_dir = self
            .wal_path
            .parent()
            .ok_or_else(|| "WAL path has no parent directory".to_string())?;
        let state_file = state_dir.join("state.json");
        let tmp_file = state_dir.join("state.json.tmp");

        // Atomic write: tmp + fsync + rename
        let write_result = (|| -> Result<(), std::io::Error> {
            let mut f = File::create(&tmp_file)?;
            f.write_all(state_json.as_bytes())?;
            f.sync_all()?;
            fs::rename(&tmp_file, &state_file)?;
            sync_directory(state_dir)?;
            Ok(())
        })();

        if let Err(e) = write_result {
            let _ = fs::remove_file(&tmp_file);
            return Err(format!("Failed to write snapshot: {}", e));
        }

        // Truncate WAL
        let file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&self.wal_path)
            .map_err(|e| format!("Failed to truncate WAL: {}", e))?;
        file.sync_all()
            .map_err(|e| format!("Failed to sync truncated WAL: {}", e))?;
        self.file = BufWriter::new(file);
        self.entry_count = 0;
        self.last_compact_time = Instant::now();

        Ok(())
    }
}

pub enum WalMessage {
    Append {
        entry: WalEntry,
        ack: oneshot::Sender<Result<(), String>>,
    },
    Compact {
        state_json: String,
        ack: oneshot::Sender<Result<(), String>>,
    },
    Shutdown {
        ack: oneshot::Sender<()>,
    },
}

#[derive(Clone)]
pub struct WalClient {
    sender: mpsc::Sender<WalMessage>,
    entry_count: Arc<AtomicU64>,
    last_compact_time: Arc<Mutex<Instant>>,
}

impl WalClient {
    pub fn open(state_path: &str) -> Result<Self, String> {
        let wal = WalWriter::open(state_path)?;
        let entry_count = Arc::new(AtomicU64::new(wal.entry_count()));
        let last_compact_time = Arc::new(Mutex::new(Instant::now()));
        let (sender, receiver) = mpsc::channel(WAL_CHANNEL_CAPACITY);

        let actor = WalActor {
            wal,
            receiver,
            entry_count: entry_count.clone(),
            last_compact_time: last_compact_time.clone(),
        };

        thread::Builder::new()
            .name("aria-wal-worker".to_string())
            .spawn(move || actor.run())
            .map_err(|e| format!("Failed to spawn WAL thread: {}", e))?;

        Ok(Self {
            sender,
            entry_count,
            last_compact_time,
        })
    }

    pub async fn append(&self, entry: WalEntry) -> Result<(), String> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.sender
            .send(WalMessage::Append { entry, ack: ack_tx })
            .await
            .map_err(|_| "WAL worker thread died".to_string())?;
        ack_rx
            .await
            .unwrap_or_else(|_| Err("WAL ack channel dropped".to_string()))
    }

    pub async fn compact(&self, state_json: String) -> Result<(), String> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.sender
            .send(WalMessage::Compact {
                state_json,
                ack: ack_tx,
            })
            .await
            .map_err(|_| "WAL worker thread died".to_string())?;
        ack_rx
            .await
            .unwrap_or_else(|_| Err("WAL ack channel dropped".to_string()))
    }

    pub async fn shutdown(&self) {
        let (ack_tx, ack_rx) = oneshot::channel();
        if self
            .sender
            .send(WalMessage::Shutdown { ack: ack_tx })
            .await
            .is_ok()
        {
            let _ = ack_rx.await;
        }
    }

    pub fn entry_count(&self) -> u64 {
        self.entry_count.load(Ordering::Relaxed)
    }

    pub fn needs_compact(&self, threshold: u64) -> bool {
        let count = self.entry_count();
        if count == 0 {
            return false;
        }
        let elapsed = self
            .last_compact_time
            .lock()
            .map(|guard| guard.elapsed().as_secs())
            .unwrap_or(0);
        count >= threshold || elapsed >= WAL_COMPACT_INTERVAL_SECS
    }

    /// Approximate number of queued WAL messages waiting behind the actor.
    pub fn queue_depth(&self) -> usize {
        WAL_CHANNEL_CAPACITY.saturating_sub(self.sender.capacity())
    }
}

struct WalActor {
    wal: WalWriter,
    receiver: mpsc::Receiver<WalMessage>,
    entry_count: Arc<AtomicU64>,
    last_compact_time: Arc<Mutex<Instant>>,
}

impl WalActor {
    fn run(mut self) {
        let mut deferred: Option<WalMessage> = None;

        loop {
            let msg = match deferred.take() {
                Some(msg) => msg,
                None => match self.receiver.blocking_recv() {
                    Some(msg) => msg,
                    None => break,
                },
            };

            match msg {
                WalMessage::Append { entry, ack } => {
                    let mut acks = Vec::with_capacity(MAX_BATCH_SIZE);
                    let mut appended = 0u64;

                    match self.wal.append_buffered(&entry) {
                        Ok(()) => {
                            acks.push(ack);
                            appended += 1;
                        }
                        Err(e) => {
                            let _ = ack.send(Err(e));
                            continue;
                        }
                    }

                    while acks.len() < MAX_BATCH_SIZE {
                        match self.receiver.try_recv() {
                            Ok(WalMessage::Append { entry, ack }) => {
                                match self.wal.append_buffered(&entry) {
                                    Ok(()) => {
                                        acks.push(ack);
                                        appended += 1;
                                    }
                                    Err(e) => {
                                        let _ = ack.send(Err(e));
                                        break;
                                    }
                                }
                            }
                            Ok(other) => {
                                deferred = Some(other);
                                break;
                            }
                            Err(mpsc::error::TryRecvError::Empty) => break,
                            Err(mpsc::error::TryRecvError::Disconnected) => break,
                        }
                    }

                    let result = self.wal.sync();
                    if result.is_ok() {
                        self.entry_count.fetch_add(appended, Ordering::Relaxed);
                    }
                    for ack in acks {
                        let _ = ack.send(result.clone());
                    }
                }
                WalMessage::Compact { state_json, ack } => {
                    let result = self.wal.compact(&state_json);
                    if result.is_ok() {
                        self.entry_count.store(0, Ordering::Relaxed);
                        if let Ok(mut guard) = self.last_compact_time.lock() {
                            *guard = Instant::now();
                        }
                    }
                    let _ = ack.send(result);
                }
                WalMessage::Shutdown { ack } => {
                    if let Err(e) = self.wal.sync() {
                        warn!(error = %e, "final WAL sync on shutdown failed");
                    }
                    let _ = ack.send(());
                    break;
                }
            }
        }
    }
}
