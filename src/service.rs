use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use crate::config::Config;
use crate::errors::{Context, Result};

pub struct ServiceManager {
    pid_file: PathBuf,
    monitor: bool,
}

impl ServiceManager {
    pub fn new(config: &Config) -> Self {
        let pid_file = config.storage.data_dir.join("clipkeeper.pid");
        Self { pid_file, monitor: false }
    }

    pub fn with_monitor(mut self, monitor: bool) -> Self {
        self.monitor = monitor;
        self
    }

    pub fn start(&self) -> Result<()> {
        if self.is_running()? {
            let pid = self.get_pid()?;
            anyhow::bail!("Service is already running (PID: {})", pid);
        }

        let _exe = std::env::current_exe()
            .context("Failed to get current executable path")?;

        #[cfg(unix)]
        {
            use daemonize::Daemonize;

            let daemonize = Daemonize::new()
                .pid_file(&self.pid_file)
                .working_directory(std::env::current_dir()
                    .context("Failed to get current directory")?);

            match daemonize.start() {
                Ok(_) => {
                    crate::app::run_service(self.monitor)?;
                }
                Err(e) => anyhow::bail!("Failed to start service: {}", e),
            }
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            use std::process::Command;

            let mut args = vec!["start", "--service"];
            if self.monitor {
                args.push("--monitor");
            }

            // CREATE_NO_WINDOW (0x08000000) + DETACHED_PROCESS (0x00000008)
            let child = Command::new(&_exe)
                .args(&args)
                .creation_flags(0x08000008)
                .spawn()
                .context("Failed to start service: could not spawn process")?;

            fs::write(&self.pid_file, child.id().to_string())
                .context("Failed to write PID file")?;
        }

        Ok(())
    }

    /// Stop the service with SIGTERM, then SIGKILL after 5s timeout (Task 12.3)
    pub fn stop(&self) -> Result<()> {
        if !self.is_running()? {
            // Clean up stale PID file
            self.cleanup_pid_file();
            anyhow::bail!("Service is not running");
        }

        let pid = self.get_pid()?;

        #[cfg(unix)]
        {
            use std::time::Instant;
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;

            // Send SIGTERM first
            match kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
                Ok(_) => {}
                Err(nix::errno::Errno::ESRCH) => {
                    self.cleanup_pid_file();
                    return Ok(());
                }
                Err(e) => anyhow::bail!("Failed to send SIGTERM: {}", e),
            }

            // Wait up to 5 seconds for graceful shutdown
            let start = Instant::now();
            let timeout = Duration::from_secs(5);
            while start.elapsed() < timeout {
                std::thread::sleep(Duration::from_millis(100));
                if !self.is_process_running(pid) {
                    self.cleanup_pid_file();
                    return Ok(());
                }
            }

            // Force kill if still running
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
            self.cleanup_pid_file();
        }

        #[cfg(windows)]
        {
            use std::process::Command;
            Command::new("taskkill")
                .args(&["/PID", &pid.to_string(), "/F"])
                .output()
                .context("Failed to kill process")?;
            self.cleanup_pid_file();
        }

        Ok(())
    }

    pub fn is_running(&self) -> Result<bool> {
        if !self.pid_file.exists() {
            return Ok(false);
        }

        let pid = match self.get_pid() {
            Ok(p) => p,
            Err(_) => {
                self.cleanup_pid_file();
                return Ok(false);
            }
        };

        if self.is_process_running(pid) {
            Ok(true)
        } else {
            // Stale PID file
            self.cleanup_pid_file();
            Ok(false)
        }
    }

    pub fn get_pid(&self) -> Result<u32> {
        let content = fs::read_to_string(&self.pid_file)
            .context("Failed to read PID file")?;
        let pid: u32 = content.trim().parse()
            .with_context(|| format!("Invalid PID: {}", content.trim()))?;
        Ok(pid)
    }

    /// Get uptime by checking PID file modification time (Task 12.4)
    pub fn get_uptime(&self) -> Option<Duration> {
        if let Ok(metadata) = fs::metadata(&self.pid_file) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    return Some(elapsed);
                }
            }
        }
        None
    }

    fn is_process_running(&self, pid: u32) -> bool {
        #[cfg(unix)]
        {
            use nix::sys::signal::kill;
            use nix::unistd::Pid;
            kill(Pid::from_raw(pid as i32), None).is_ok()
        }
        #[cfg(windows)]
        {
            use std::process::Command;
            Command::new("tasklist")
                .args(&["/FI", &format!("PID eq {}", pid)])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
                .unwrap_or(false)
        }
    }

    fn cleanup_pid_file(&self) {
        let _ = fs::remove_file(&self.pid_file);
    }

    /// Get the path to the PID file
    pub fn pid_file_path(&self) -> &PathBuf {
        &self.pid_file
    }
}
