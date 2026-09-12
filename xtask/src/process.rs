#[cfg(unix)]
use std::time::{Duration, Instant};
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, ExitStatus, Stdio},
    thread::{self, JoinHandle},
};

use anyhow::{Context, Result, anyhow};
use process_wrap::std::{ChildWrapper, CommandWrap};

/// Own a whole process group/job, including pnpm's Node and shell descendants.
/// Unlike dropping a std::process::Child, dropping this owner always stops it.
pub struct Process {
    child: Box<dyn ChildWrapper>,
    name: &'static str,
    stopped: bool,
}

impl Process {
    pub fn spawn(name: &'static str, mut command: Command) -> Result<Self> {
        eprintln!("[xtask] starting {name}");
        command.stdin(Stdio::null());
        let mut command = CommandWrap::from(command);
        #[cfg(unix)]
        command.wrap(process_wrap::std::ProcessGroup::leader());
        #[cfg(windows)]
        command.wrap(process_wrap::std::JobObject);
        Ok(Self {
            child: command
                .spawn()
                .with_context(|| format!("cannot start {name}"))?,
            name,
            stopped: false,
        })
    }

    pub fn poll(&mut self) -> Result<Option<ExitStatus>> {
        self.child
            .try_wait()
            .with_context(|| format!("cannot poll {}", self.name))
    }

    pub fn stop(&mut self) -> Result<()> {
        if self.stopped {
            return Ok(());
        }
        #[cfg(unix)]
        {
            // SIGTERM is 15 on the supported Unix platforms. The wrapper addresses the group,
            // not just the pnpm/Cargo parent; the backend handles SIGTERM gracefully.
            // ESRCH (3) means the group has already exited.
            if let Err(error) = self.child.signal(15)
                && error.raw_os_error() != Some(3)
            {
                return Err(error).with_context(|| format!("cannot terminate {}", self.name));
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            while self.poll()?.is_none() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(50));
            }
        }
        // Also clean up descendants after the leader exits. On Windows this closes the job.
        if let Err(error) = self.child.start_kill()
            && self.poll()?.is_none()
        {
            return Err(error).with_context(|| format!("cannot kill {}", self.name));
        }
        self.child
            .wait()
            .with_context(|| format!("cannot reap {}", self.name))?;
        self.stopped = true;
        Ok(())
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("[xtask] cleanup {}: {error:#}", self.name);
        }
    }
}

pub struct Build {
    pub process: Process,
    reader: Option<JoinHandle<Result<Option<PathBuf>>>>,
}

impl Build {
    pub fn start(mut command: Command) -> Result<Self> {
        command
            .arg("--message-format=json-render-diagnostics")
            .stdout(Stdio::piped());
        let mut process = Process::spawn("Rust build", command)?;
        let stdout = process
            .child
            .stdout()
            .take()
            .context("Cargo stdout is missing")?;
        let reader = thread::spawn(move || {
            let mut executable = None;
            for line in BufReader::new(stdout).lines() {
                let line = line?;
                let message: serde_json::Value = serde_json::from_str(&line)
                    .with_context(|| format!("invalid Cargo build message: {line}"))?;
                if message["reason"] == "compiler-artifact"
                    && message["target"]["name"] == "llm-bridge"
                    && let Some(path) = message["executable"].as_str()
                {
                    executable = Some(PathBuf::from(path));
                }
            }
            Ok(executable)
        });
        Ok(Self {
            process,
            reader: Some(reader),
        })
    }

    pub fn executable(&mut self) -> Result<Option<PathBuf>> {
        self.reader
            .take()
            .context("Cargo output already collected")?
            .join()
            .map_err(|_| anyhow!("Cargo output reader panicked"))?
    }
}

impl Drop for Build {
    fn drop(&mut self) {
        // Stop before joining: the reader can be blocked on a running Cargo process's stdout.
        if let Err(error) = self.process.stop() {
            eprintln!("[xtask] stopping build: {error:#}");
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Read;
    use std::sync::mpsc;

    #[test]
    fn dropping_process_closes_descendant_stdout_even_when_it_ignores_term() {
        let mut command = Command::new("sh");
        command
            .args(["-c", "(trap '' TERM; echo ready; exec sleep 60) & wait"])
            .stdout(Stdio::piped());
        let mut process = Process::spawn("descendant cleanup regression", command).unwrap();
        let mut stdout = BufReader::new(process.child.stdout().take().unwrap());
        let mut ready = String::new();
        stdout.read_line(&mut ready).unwrap();
        assert_eq!(ready.trim(), "ready");

        drop(process);
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut remaining = String::new();
            tx.send(stdout.read_to_string(&mut remaining)).unwrap();
        });
        rx.recv_timeout(Duration::from_secs(2))
            .expect("a surviving descendant still owns stdout")
            .unwrap();
    }
}
