use std::sync::Arc;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct UploadProgress {
    pub bytes_uploaded: u64,
    pub total_bytes: u64,
    pub provider: String,
    #[allow(dead_code)]
    pub percentage: f64,
}

pub trait ProgressCallback: Send + Sync {
    fn call(&self, progress: &UploadProgress);
    fn finish(&self) {}
}

#[derive(Default)]
#[allow(dead_code)]
pub struct NoOpCallback;

impl ProgressCallback for NoOpCallback {
    fn call(&self, _: &UploadProgress) {}
    fn finish(&self) {}
}

#[derive(Clone)]
pub struct VerboseProgressCallback {
    enabled: bool,
    last_output: Arc<Mutex<String>>,
}

impl VerboseProgressCallback {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            last_output: Arc::new(Mutex::new(String::new())),
        }
    }

    fn clear_line(&self) {
        let last = self.last_output.lock().unwrap();
        if !last.is_empty() {
            eprint!("\r{}\r", " ".repeat(last.len()));
        }
    }
}

impl ProgressCallback for VerboseProgressCallback {
    fn call(&self, progress: &UploadProgress) {
        if !self.enabled {
            return;
        }

        let percentage = if progress.total_bytes > 0 {
            (progress.bytes_uploaded as f64 / progress.total_bytes as f64) * 100.0
        } else {
            0.0
        };

        let output = format!(
            "[{:>3.0}%] {:>10} / {:>10} bytes - {}",
            percentage,
            format_bytes(progress.bytes_uploaded),
            format_bytes(progress.total_bytes),
            progress.provider
        );

        self.clear_line();
        eprint!("{}", output);
        *self.last_output.lock().unwrap() = output;
    }

    fn finish(&self) {
        if !self.enabled {
            return;
        }
        self.clear_line();
        eprintln!();
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[derive(Clone)]
pub struct ProgressTracker {
    total_bytes: u64,
    bytes_uploaded: Arc<std::sync::atomic::AtomicU64>,
    callback: Arc<dyn ProgressCallback>,
    provider: String,
}

impl ProgressTracker {
    pub fn new(total_bytes: u64, callback: Arc<dyn ProgressCallback>, provider: String) -> Self {
        Self {
            total_bytes,
            bytes_uploaded: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            callback,
            provider,
        }
    }

    pub fn add_progress(&self, bytes: u64) {
        let current = self
            .bytes_uploaded
            .fetch_add(bytes, std::sync::atomic::Ordering::SeqCst);
        let total = current + bytes;

        let progress = UploadProgress {
            bytes_uploaded: total,
            total_bytes: self.total_bytes,
            provider: self.provider.clone(),
            percentage: if self.total_bytes > 0 {
                (total as f64 / self.total_bytes as f64) * 100.0
            } else {
                0.0
            },
        };

        self.callback.call(&progress);
    }

    pub fn finish(&self) {
        let progress = UploadProgress {
            bytes_uploaded: self.total_bytes,
            total_bytes: self.total_bytes,
            provider: self.provider.clone(),
            percentage: 100.0,
        };
        self.callback.call(&progress);
        self.callback.finish();
    }
}
