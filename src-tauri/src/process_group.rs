use std::io;
use std::process::{Child, Command};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// Configure the child before spawn so Unix descendants share a private process group.
///
/// Windows deliberately does not use CREATE_SUSPENDED here: std::process::Command does
/// not expose the primary thread handle needed to assign a suspended process safely. The
/// Windows implementation therefore attaches a JobObject immediately after spawn and keeps
/// taskkill as a fallback for the small pre-assignment race.
pub(crate) fn configure_command(command: &mut Command) {
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    #[cfg(windows)]
    let _ = command;
}

#[cfg(unix)]
#[derive(Debug)]
pub(crate) struct ProcessGroup {
    pgid: libc::pid_t,
}

#[cfg(windows)]
#[derive(Debug)]
pub(crate) struct ProcessGroup {
    pid: u32,
    // Store the opaque Windows handle as an integer so ProcessGroup can move into
    // the worker thread; it is cast back only at the FFI boundary.
    job: Option<usize>,
}

#[cfg(not(any(unix, windows)))]
#[derive(Debug)]
pub(crate) struct ProcessGroup {
    pid: u32,
}

impl ProcessGroup {
    /// Bind a freshly spawned child to its process group/job.
    pub(crate) fn attach(child: &Child) -> io::Result<Self> {
        #[cfg(unix)]
        {
            let pid = child.id() as libc::pid_t;
            let pgid = unsafe { libc::getpgid(pid) };
            if pgid < 0 {
                // setsid() already made pid the group leader. Keep a pid fallback if the
                // lookup races with an early exit; terminate/kill treat ESRCH as success.
                return Ok(Self { pgid: pid });
            }
            return Ok(Self { pgid });
        }

        #[cfg(windows)]
        {
            Self::attach_windows(child)
        }

        #[cfg(not(any(unix, windows)))]
        {
            Ok(Self { pid: child.id() })
        }
    }

    /// Construct a handle that deliberately uses the platform fallback path.
    pub(crate) fn fallback(pid: u32) -> Self {
        #[cfg(unix)]
        {
            Self {
                pgid: pid as libc::pid_t,
            }
        }

        #[cfg(windows)]
        {
            Self { pid, job: None }
        }

        #[cfg(not(any(unix, windows)))]
        {
            Self { pid }
        }
    }

    #[cfg(test)]
    pub(crate) fn is_fallback(&self) -> bool {
        #[cfg(windows)]
        {
            self.job.is_none()
        }

        #[cfg(not(windows))]
        {
            false
        }
    }

    /// Request graceful termination. Unix sends SIGTERM to the whole process group.
    /// Windows has no portable SIGTERM equivalent, so taskkill /T without /F is used;
    /// the bounded grace period is followed by JobObject/taskkill force termination.
    pub(crate) fn terminate(&self, child: &Child) -> io::Result<()> {
        #[cfg(unix)]
        {
            if self.pgid <= 0 {
                return child_terminate(child);
            }
            let result = unsafe { libc::kill(-self.pgid, libc::SIGTERM) };
            if result == 0 || last_errno() == libc::ESRCH {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        }

        #[cfg(windows)]
        {
            let _ = child;
            taskkill(self.pid, false)
        }

        #[cfg(not(any(unix, windows)))]
        {
            child_terminate(child)
        }
    }

    /// Force termination of the process tree/group. This must be called only after the
    /// caller's finite grace period has elapsed.
    pub(crate) fn kill(&self, child: &mut Child) -> io::Result<()> {
        #[cfg(unix)]
        {
            if self.pgid > 0 {
                let result = unsafe { libc::kill(-self.pgid, libc::SIGKILL) };
                if result == 0 || last_errno() == libc::ESRCH {
                    return Ok(());
                }
            }
            child_kill(child)
        }

        #[cfg(windows)]
        {
            if let Some(job) = self.job {
                let result = unsafe {
                    windows_sys::Win32::System::JobObjects::TerminateJobObject(
                        job as windows_sys::Win32::Foundation::HANDLE,
                        1,
                    )
                };
                if result != 0 {
                    return Ok(());
                }
            }
            taskkill(self.pid, true).or_else(|_| child_kill(child))
        }

        #[cfg(not(any(unix, windows)))]
        {
            child_kill(child)
        }
    }

    #[cfg(windows)]
    fn attach_windows(child: &Child) -> io::Result<Self> {
        use std::mem::{size_of, zeroed};
        use std::ptr::null;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_BASIC_LIMIT_INFORMATION,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
        };

        let job = unsafe { CreateJobObjectW(null(), null()) };
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }

        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        info.BasicLimitInformation = JOBOBJECT_BASIC_LIMIT_INFORMATION {
            LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            ..unsafe { zeroed() }
        };
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            unsafe { CloseHandle(job) };
            return Err(io::Error::last_os_error());
        }

        let process = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, child.id()) };
        if process.is_null() {
            unsafe { CloseHandle(job) };
            return Err(io::Error::last_os_error());
        }
        let assigned = unsafe { AssignProcessToJobObject(job, process) };
        unsafe { CloseHandle(process) };
        if assigned == 0 {
            unsafe { CloseHandle(job) };
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            pid: child.id(),
            job: Some(job as usize),
        })
    }
}

#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {}
}

#[cfg(windows)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(
                    job as windows_sys::Win32::Foundation::HANDLE,
                )
            };
        }
    }
}

#[cfg(not(any(unix, windows)))]
impl Drop for ProcessGroup {
    fn drop(&mut self) {}
}

#[cfg(not(windows))]
fn child_terminate(child: &Child) -> io::Result<()> {
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
        if result == 0 || last_errno() == libc::ESRCH {
            return Ok(());
        }
        return Err(io::Error::last_os_error());
    }

    #[cfg(windows)]
    {
        return taskkill(child.id(), false);
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "process termination unsupported",
        ))
    }
}

fn child_kill(child: &mut Child) -> io::Result<()> {
    child.kill()
}

#[cfg(unix)]
fn last_errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

#[cfg(windows)]
fn taskkill(pid: u32, force: bool) -> io::Result<()> {
    let pid = pid.to_string();
    let mut command = Command::new("taskkill");
    command.args(["/PID", &pid, "/T"]);
    if force {
        command.arg("/F");
    }
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(if force {
            "taskkill force fallback failed"
        } else {
            "taskkill graceful fallback failed"
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_handle_is_constructible_for_missing_process() {
        let group = ProcessGroup::fallback(u32::MAX);
        #[cfg(windows)]
        assert!(group.is_fallback());
        #[cfg(not(windows))]
        assert!(!group.is_fallback());
    }
}
