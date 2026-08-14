use crate::jobs::errors::{JobError, JobResult};
use crate::jobs::job::{Job, JobStatus};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// File-backed persistent job store.
/// Jobs are persisted as individual JSON files under `.nabu/jobs/{status}/{id}.json`.
/// This ensures the queue survives application shutdown, crashes, and power loss.
pub struct JobStore {
    base_path: PathBuf,
    inner: Mutex<JobStoreInner>,
}

struct JobStoreInner {
    /// Status_dir → { job_id → Job }
    jobs: HashMap<String, HashMap<String, Job>>,
    /// All jobs indexed by ID for fast lookup
    by_id: HashMap<String, Job>,
}

impl JobStore {
    /// Create a new JobStore rooted at the given base path.
    /// The store will create `.nabu/jobs/` directory structure if it doesn't exist.
    pub fn new(base_path: impl Into<PathBuf>) -> JobResult<Self> {
        let base_path: PathBuf = base_path.into();
        let jobs_path = base_path.join(".nabu").join("jobs");

        // Ensure directory structure exists
        fs::create_dir_all(&jobs_path)?;

        // Create status subdirectories
        for status in &[
            "queued",
            "running",
            "completed",
            "failed",
            "cancelled",
            "scheduled",
        ] {
            fs::create_dir_all(jobs_path.join(status))?;
        }

        // Create the binary-blob storage sub-directory for persisted capture
        // payloads that cannot be embedded directly in the JSON job payload.
        fs::create_dir_all(jobs_path.join("blobs"))?;

        let store = Self {
            base_path: jobs_path,
            inner: Mutex::new(JobStoreInner {
                jobs: HashMap::new(),
                by_id: HashMap::new(),
            }),
        };

        // Load existing jobs from disk
        store.load_all()?;

        Ok(store)
    }

    /// Persist a job to disk.
    pub fn store(&self, job: &Job) -> JobResult<()> {
        let status_dir = job.status.label();
        let job_path = self
            .base_path
            .join(status_dir)
            .join(format!("{}.json", job.id));

        let json = serde_json::to_string_pretty(job)?;
        let mut file = fs::File::create(&job_path)?;
        file.write_all(json.as_bytes())?;

        // Update in-memory indexes
        let mut inner = self.inner.lock().unwrap();
        inner
            .jobs
            .entry(status_dir.to_string())
            .or_default()
            .insert(job.id.to_string(), job.clone());
        inner.by_id.insert(job.id.to_string(), job.clone());

        Ok(())
    }

    /// Load a single job by ID.
    pub fn load(&self, job_id: &str) -> JobResult<Option<Job>> {
        // Check memory first
        let inner = self.inner.lock().unwrap();
        if let Some(job) = inner.by_id.get(job_id) {
            return Ok(Some(job.clone()));
        }

        // Fall through to disk lookup
        drop(inner);
        self.load_from_disk(job_id)
    }

    /// Load all jobs with a given status.
    pub fn load_by_status(&self, status: JobStatus) -> JobResult<Vec<Job>> {
        let status_dir = status.label();
        let mut jobs = Vec::new();

        let dir_path = self.base_path.join(status_dir);
        if !dir_path.exists() {
            return Ok(jobs);
        }

        let entries = fs::read_dir(&dir_path)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let content = fs::read_to_string(&path)?;
                if let Ok(job) = serde_json::from_str::<Job>(&content) {
                    jobs.push(job);
                }
            }
        }

        Ok(jobs)
    }

    /// Move a job from one status to another (atomically).
    pub fn move_job(&self, job_id: &str, from: JobStatus, to: JobStatus) -> JobResult<Job> {
        let mut job = self
            .load(job_id)?
            .ok_or_else(|| JobError::NotFound(job_id.to_string()))?;

        // Remove from old status directory
        let old_path = self
            .base_path
            .join(from.label())
            .join(format!("{}.json", job_id));
        let _ = fs::remove_file(&old_path);

        // Update status
        job.status = to.clone();
        self.store(&job)?;

        Ok(job)
    }

    /// Remove a job entirely from disk and memory.
    pub fn remove(&self, job_id: &str) -> JobResult<()> {
        // Remove from all status directories
        for status in &[
            "queued",
            "running",
            "completed",
            "failed",
            "cancelled",
            "scheduled",
        ] {
            let path = self.base_path.join(status).join(format!("{}.json", job_id));
            let _ = fs::remove_file(&path);
        }

        // Remove from memory
        let mut inner = self.inner.lock().unwrap();
        for jobs in inner.jobs.values_mut() {
            jobs.remove(job_id);
        }
        inner.by_id.remove(job_id);

        Ok(())
    }

    /// Count jobs with a given status.
    pub fn count(&self, status: JobStatus) -> JobResult<usize> {
        let status_dir = status.label();
        let dir_path = self.base_path.join(status_dir);
        if !dir_path.exists() {
            return Ok(0);
        }

        let count = fs::read_dir(&dir_path)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
            .count();

        Ok(count)
    }

    /// Total count of all jobs (non-terminal).
    pub fn active_count(&self) -> JobResult<usize> {
        let queued = self.count(JobStatus::Queued)?;
        let running = self.count(JobStatus::Running)?;
        let scheduled = self.count(JobStatus::Scheduled)?;
        Ok(queued + running + scheduled)
    }

    /// Load all jobs from disk into memory.
    fn load_all(&self) -> JobResult<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.jobs.clear();
        inner.by_id.clear();

        for status in &[
            "queued",
            "running",
            "completed",
            "failed",
            "cancelled",
            "scheduled",
        ] {
            let dir_path = self.base_path.join(status);
            if !dir_path.exists() {
                continue;
            }

            let mut status_jobs = HashMap::new();
            if let Ok(entries) = fs::read_dir(&dir_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("json") {
                        if let Ok(content) = fs::read_to_string(&path) {
                            if let Ok(job) = serde_json::from_str::<Job>(&content) {
                                let id = job.id.to_string();
                                status_jobs.insert(id.clone(), job.clone());
                                inner.by_id.insert(id, job);
                            }
                        }
                    }
                }
            }

            inner.jobs.insert(status.to_string(), status_jobs);
        }

        Ok(())
    }

    /// Load a job from disk by scanning all status directories.
    fn load_from_disk(&self, job_id: &str) -> JobResult<Option<Job>> {
        for status in &[
            "queued",
            "running",
            "completed",
            "failed",
            "cancelled",
            "scheduled",
        ] {
            let path = self.base_path.join(status).join(format!("{}.json", job_id));
            if path.exists() {
                let content = fs::read_to_string(&path)?;
                let job: Job = serde_json::from_str(&content)?;
                return Ok(Some(job));
            }
        }
        Ok(None)
    }

    /// Path to the jobs directory.
    pub fn path(&self) -> &Path {
        &self.base_path
    }

    /// Absolute path to the blobs directory used for persisting binary
    /// capture payloads that are too large (or not safely representable) in
    /// the JSON job payload.
    pub fn blobs_path(&self) -> PathBuf {
        self.base_path.join("blobs")
    }

    /// Persist a binary blob for a capture, keyed by the object id.
    ///
    /// The bytes are written to `<store>/blobs/{id}` and the absolute
    /// path to the file is returned.  This path is durable across process
    /// restarts and can be stored in a [`Job`]'s `content_payload` as a
    /// `blob_path` reference.
    pub fn store_blob(
        &self,
        id: &str,
        data: &[u8],
    ) -> JobResult<String> {
        let blobs_dir = self.base_path.join("blobs");
        fs::create_dir_all(&blobs_dir)?;
        let blob_path = blobs_dir.join(id);
        fs::write(&blob_path, data)?;
        // Return an absolute, canonicalised path so the reference survives
        // regardless of the current working directory.
        let canonical = blob_path
            .canonicalize()
            .unwrap_or_else(|_| blob_path.clone());
        Ok(canonical.to_string_lossy().to_string())
    }

    /// Load a binary blob from its absolute path.
    ///
    /// Returns `Err` when the file is missing or unreadable, so that
    /// callers can fail explicitly rather than silently producing an empty
    /// object.
    ///
    /// The path is validated to be within the store's `blobs/` directory
    /// to prevent arbitrary file reads.
    pub fn load_blob(&self, path: &str) -> JobResult<Vec<u8>> {
        let path = std::path::Path::new(path);
        let canonical = path.canonicalize().map_err(|e| JobError::Persistence(e.to_string()))?;
        let blobs_abs = self
            .blobs_path()
            .canonicalize()
            .map_err(|e| JobError::Persistence(e.to_string()))?;
        if !canonical.starts_with(&blobs_abs) {
            return Err(JobError::Persistence(format!(
                "security: blob path {} is outside the blobs directory {}",
                canonical.display(),
                blobs_abs.display()
            )));
        }
        fs::read(&canonical).map_err(JobError::from)
    }

    /// Clear all persisted jobs and binary blobs (for testing).
    pub fn clear_all(&self) -> JobResult<()> {
        for status in &[
            "queued",
            "running",
            "completed",
            "failed",
            "cancelled",
            "scheduled",
        ] {
            let dir_path = self.base_path.join(status);
            if dir_path.exists() {
                for entry in fs::read_dir(&dir_path).into_iter().flatten().flatten() {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }

        // Also remove binary blob files.
        let blobs_dir = self.base_path.join("blobs");
        if blobs_dir.exists() {
            for entry in fs::read_dir(&blobs_dir).into_iter().flatten().flatten() {
                let _ = fs::remove_file(entry.path());
            }
        }

        let mut inner = self.inner.lock().unwrap();
        inner.jobs.clear();
        inner.by_id.clear();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::priority::Priority;

    #[test]
    fn test_store_and_load_job() {
        let dir = tempfile::tempdir().unwrap();
        let store = JobStore::new(dir.path()).unwrap();

        let job = Job::new(
            crate::jobs::job::JobType::Ocr,
            serde_json::json!({"path": "/test.pdf"}),
            "ocr_processor",
        )
        .with_priority(Priority::High);

        store.store(&job).unwrap();
        let loaded = store.load(&job.id.to_string()).unwrap().unwrap();
        assert_eq!(loaded.id, job.id);
        assert_eq!(loaded.priority, Priority::High);
    }

    #[test]
    fn test_move_job() {
        let dir = tempfile::tempdir().unwrap();
        let store = JobStore::new(dir.path()).unwrap();

        let job = Job::new(
            crate::jobs::job::JobType::Whisper,
            serde_json::json!({"audio": "/test.mp3"}),
            "whisper_processor",
        );
        store.store(&job).unwrap();

        let moved = store
            .move_job(&job.id.to_string(), JobStatus::Queued, JobStatus::Running)
            .unwrap();
        assert_eq!(moved.status, JobStatus::Running);
    }

    #[test]
    fn test_survives_restart() {
        let dir = tempfile::tempdir().unwrap();

        let job;
        {
            let store = JobStore::new(dir.path()).unwrap();
            job = Job::new(
                crate::jobs::job::JobType::Ocr,
                serde_json::json!({"path": "/test.pdf"}),
                "ocr_processor",
            );
            store.store(&job).unwrap();
        } // store dropped (simulating shutdown)

        {
            let store = JobStore::new(dir.path()).unwrap();
            let loaded = store.load(&job.id.to_string()).unwrap().unwrap();
            assert_eq!(loaded.id, job.id);
        }
    }

    #[test]
    fn test_store_and_load_blob() {
        let dir = tempfile::tempdir().unwrap();
        let store = JobStore::new(dir.path()).unwrap();

        let data = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a];
        let blob_path = store.store_blob("test-object", &data).unwrap();

        // The blob should be stored inside the store's blobs/ directory.
        let blobs_dir = store.blobs_path();
        let canonical_blob = std::path::Path::new(&blob_path).canonicalize().unwrap();
        let canonical_blobs = blobs_dir.canonicalize().unwrap();
        assert!(
            canonical_blob.starts_with(&canonical_blobs),
            "blob should be in blobs dir: {} vs {}",
            canonical_blob.display(),
            canonical_blobs.display()
        );

        let loaded = store.load_blob(&blob_path).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_blob_path_outside_store_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = JobStore::new(dir.path()).unwrap();

        // Try to load a path outside the blobs directory.
        let outside = dir.path().join("secret.txt");
        std::fs::write(&outside, b"secret").unwrap();
        let result = store.load_blob(&outside.to_string_lossy());
        assert!(result.is_err(), "should reject paths outside blobs dir");
    }

    #[test]
    fn test_job_with_content_payload_survives_restart() {
        let dir = tempfile::tempdir().unwrap();

        let job;
        {
            let store = JobStore::new(dir.path()).unwrap();
            let blob_path = store.store_blob("obj-1", &[0x00, 0x01, 0x02]).unwrap();
            job = Job::new(
                crate::jobs::job::JobType::Ocr,
                serde_json::json!({
                    "object_id": "00000000-0000-0000-0000-000000000001",
                    "object_type": "screenshot",
                    "title": "Test",
                }),
                "ocr_processor",
            )
            .with_content_payload(crate::jobs::job::ContentPayload::Binary {
                mime_type: "image/png".to_string(),
                filename: None,
                blob_path,
            });
            store.store(&job).unwrap();
        }

        {
            let store = JobStore::new(dir.path()).unwrap();
            let loaded = store.load(&job.id.to_string()).unwrap().unwrap();
            assert_eq!(loaded.id, job.id);
            assert!(loaded.content_payload.is_some());
            match &loaded.content_payload {
                Some(crate::jobs::job::ContentPayload::Binary {
                    mime_type,
                    blob_path,
                    ..
                }) => {
                    assert_eq!(mime_type, "image/png");
                    // Blob file should still be readable after restart
                    let blob = std::fs::read(blob_path).unwrap();
                    assert_eq!(blob, vec![0x00, 0x01, 0x02]);
                }
                _ => panic!("expected Binary content payload"),
            }
        }
    }
}
