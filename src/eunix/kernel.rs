use crate::eunix::devfs::DeviceFilesystem;
use crate::eunix::binfs::BinFilesytem;
use crate::eunix::fs::{FileDescription, FileDescriptor, VFS, OpenMode, MountedFilesystem, OpenFlags};
use crate::*;
use crate::machine::{MachineDeviceTable, VirtualDeviceType};
use std::collections::BTreeMap;

use super::fs::{AddressSize, OpenFlags,  Filesystem, FilesystemType, VDirectory, Id, VINode};
use super::virtfs::VirtFsFilesystem;

pub type Args = Vec<String>;

#[derive(Debug, PartialEq, Eq)]
pub enum Errno {
  /// Permission denied
  EACCES(&'static str),
  /// Operation not permitted
  EPERM(&'static str),
  /// Is a directory
  EISDIR(&'static str),
  /// Not a directory
  ENOTDIR(&'static str),
  /// Name too long
  ENAMETOOLONG(&'static str),
  /// Not implemented
  ENOSYS(&'static str),
  /// No such entity
  ENOENT(&'static str),
  /// I/O Error
  EIO(&'static str),
  /// Invalid argument
  EINVAL(&'static str),
  /// Illegal byte sequence
  EILSEQ(&'static str),
  /// No such process
  ESRCH(&'static str),
  /// Bad filesystem (not standart)
  EBADFS(&'static str),
}

const ROOT_UID: Id = 0;

#[derive(Debug)]
pub struct Process {
  // 0 -> stdin, 1 -> stdout, 2 -> stderr, 3.. -> user-opened
  pub file_descriptors: BTreeMap<FileDescriptor, FileDescription>,
  // User id
  pub uid: Id,
  /// Parent pid
  pub ppid: AddressSize,
  pub pid: AddressSize,
  pub binary: String,
}

impl Process {
  fn new(bin_pathname: &str, pid: AddressSize) -> Self {
    let mut process = Self {
      file_descriptors: BTreeMap::new(),
      uid: ROOT_UID,
      ppid: 0,
      pid,
      binary: String::from(bin_pathname),
    };

    process
  }

  fn with_ppid(mut self, pid: u32) -> Self {
    self.ppid = pid;
    self
  }

  fn with_uid(mut self, uid: Id) -> Self {
    self.uid = uid;
    self
  }

}

#[derive(Debug, Clone)]
pub struct KernelDeviceTable {
  /// `realpath -> (dev_type, mounted_pathname)` 
  pub devices: BTreeMap<String, (VirtualDeviceType, Option<String>)>
}
impl From<MachineDeviceTable> for KernelDeviceTable {
  fn from(mach_dev_table: MachineDeviceTable) -> Self {
    Self {
      devices: mach_dev_table.devices
        .iter()
        .map(|(realpath, dev_type)| (realpath.to_owned(), (dev_type.to_owned(), Option::<String>::None)))
        .collect(),
    }
  }
}

#[derive(Debug)]
pub struct Kernel {
  pub vfs: VFS,
  pub processes: BTreeMap<AddressSize, Process>,
  pub current_process_id: AddressSize,
  pub device_table: KernelDeviceTable,
  // registered_filesystems: BTreeMap<>,
}

pub struct KernelParams {
  init: String,
}

impl Kernel {
  pub fn new(devices: &MachineDeviceTable, params: KernelParams) -> Self {
    let KernelParams {
      init,
    } = params;

    let mut kernel = Self {
      vfs: VFS {
        mount_points: BTreeMap::new(),
        open_files: BTreeMap::new(),
      },
      processes: BTreeMap::new(),
      current_process_id: 0,
      device_table: devices.clone().into(),
    };

    // let init_pid = kernel.allocate_pid();
    // let init_proc = Process::new(init.as_str())
    //   .with_ppid(kernel.current_process_id())
    //   .with_pid(init_pid)
    //   .with_uid(ROOT_UID);
      
    let init_proc = kernel.spawn_process("/bin/init");

    // kernel.exec(init.as_str(), Vec::new());

    todo!()
  }
  pub fn devices(&self) -> &KernelDeviceTable {
    &self.device_table
  }
  pub fn current_process_id(&self) -> u32 {
    self.current_process_id
  }
  pub fn vfs(&self) -> &VFS {
    &self.vfs
  }
  pub fn processes(&self) -> &BTreeMap<AddressSize, Process> {
    &self.processes
  }

  fn allocate_pid(&self) -> AddressSize {
    self.current_process_id() + 1
  }

  fn open_stdio_files(&self, process: &mut Process) -> Result<(), Errno> {
    // Ensure that /proc filesystem exists
    self.vfs.mount_points.get("/proc").ok_or(Errno::ENOENT("Kernel::open_stdio_files: cannot open stdio files, /proc is not mounted"))?;

    // Identify and create /proc/{pid} and /proc/{pid}/fd,
    // ignoring if already exists
    let process_pathname = format!("/proc/{}", process.pid);
    let process_fd_pathname = format!("{}/fd", process_pathname);
    self.vfs.create_dir(&process_pathname)
      .and_then(|process| self.vfs.create_dir(&process_fd_pathname));

    // Create /proc/{pid}/fd/0, /proc/{pid}/fd/1, /proc/{pid}/fd/2
    let stdin_pathname = format!("{}/{}", process_fd_pathname, 0);
    let stdout_pathname = format!("{}/{}", process_fd_pathname, 1);
    let stderr_pathname = format!("{}/{}", process_fd_pathname, 2);
    let stdin_vinode = self.vfs.create_file(stdin_pathname.as_str())?;
    let stdout_vinode = self.vfs.create_file(stdout_pathname.as_str())?;
    let stderr_vinode = self.vfs.create_file(stderr_pathname.as_str())?;

    // Actually insert all 3 stdio files as opened to process' fd table
    process.file_descriptors.insert(0, FileDescription {
      vinode: stdin_vinode,
      flags: OpenFlags::new(OpenMode::ReadWrite, true, false),
      pathname: Some(stdin_pathname),
    });
    
    process.file_descriptors.insert(0, FileDescription {
      vinode: stdout_vinode,
      flags: OpenFlags::new(OpenMode::ReadWrite, true, false),
      pathname: Some(stdout_pathname),
    });

    process.file_descriptors.insert(0, FileDescription {
      vinode: stderr_vinode,
      flags: OpenFlags::new(OpenMode::ReadWrite, true, false),
      pathname: Some(stderr_pathname),
    });

    Ok(())
  }

  /// Create new process, allocate new pid and set it to be current one,
  /// and insert new process to the process table
  fn spawn_process(&mut self, bin_pathname: &str) -> Result<Process, Errno> {
    // Parent process id - current process, lul
    let ppid = self.current_process_id();

    // Set current pid to newly allocated one - for spawned process
    self.current_process_id = self.allocate_pid();

    // Create new process
    let process = Process::new(bin_pathname, self.current_process_id)
      .with_ppid(ppid)
      .with_uid(ROOT_UID);

    // Insert it to processes table
    self.processes.insert(self.current_process_id, process);

    Ok(process)
  }
}

impl Kernel {
  pub fn start() {

  }
}

impl Kernel {
  fn exec(&self, pathname: &str, argv: Args) -> Result<AddressSize, Errno> {
      todo!()
  }
  pub fn open(&mut self, pathname: &str, flags: OpenFlags) -> Result<FileDescriptor, Errno> {
    let current_process = self
      .processes
      .get_mut(self.current_process_id as usize)
      .ok_or(Errno::ESRCH("open: cannot get current process"))?; 
    
    let vinode = self.vfs.lookup_path(pathname)?;
    let file_description = FileDescription {
      vinode,
      flags,
      pathname: pathname.to_owned(),
    };

    current_process.file_descriptors.insert(
      current_process.file_descriptors.len() as FileDescriptor,
      file_description.to_owned()
    );

    // We know that `len()` is at least 1
    Ok((current_process.file_descriptors.len() - 1) as FileDescriptor)
  }

  pub fn close(&mut self, file_descriptor: FileDescriptor) -> Result<(), Errno> {
    let current_process = self.processes
      .get_mut(self.current_process_id as usize)
      .ok_or(Errno::ESRCH("open: cannot get current process"))?; 
    
    current_process.file_descriptors.remove(&file_descriptor);

    Ok(())
  }

  pub fn read(&self, file_descriptor: FileDescriptor, count: AddressSize) -> Result<Vec<u8>, Errno> {
    todo!();
  }
  pub fn write(&mut self, file_descriptor: FileDescriptor, buffer: Vec<u8>) -> Result<AddressSize, Errno> {
    todo!();
  }
  pub fn chmod(&mut self, file_descriptor: FileDescriptor, new_perms: Vec<u8>) -> Result<(), Errno> {
    todo!();
  }
  pub fn getdents(&mut self, file_descriptor: FileDescriptor) -> Result<VDirectory, Errno> {
    let process = self.processes.get(self.current_process_id() as usize).ok_or(Errno::ESRCH("cannot get current process"))?;
    let FileDescription {
      vinode: _inode,
      flags,
      pathname,
    } = process.file_descriptors.get(&file_descriptor).ok_or(Errno::ENOENT("no such file descriptor"))?;

    // Guard for OpenMode
    match flags.mode() {
      OpenMode::Write => return Err(Errno::EACCES("getdents: permission denied")),
      OpenMode::ReadWrite | OpenMode::Read => (),
    }

    self.vfs.read_dir(pathname)
  }
  pub fn mount(&mut self, source: &str, target: &str, fs_type: FilesystemType) -> Result<(), Errno> {
    if let Some(_) = self.vfs.mount_points.get(target) {
      return Err(Errno::EINVAL("mount point already taken"))
    }

    let mounted_fs = match fs_type {
      FilesystemType::e5fs => {
        let (mount_point, internal_path) = self.vfs.match_mount_point(source)?;
        let mounted_fs = self.vfs.mount_points.get_mut(&mount_point).expect("VFS::lookup_path: we know that mount_point exist");  

        let realpath = if mounted_fs.r#type == FilesystemType::devfs {
          let devfs = mounted_fs.driver
            .as_any()
            .downcast_ref::<DeviceFilesystem>()
            .expect("we know that mounted_fs.driver === instanceof DeviceFilesystem");

          devfs.device_by_path(&internal_path)?
        } else {
          return Err(Errno::EINVAL("source is not a device"));
        };

        // Instantiate new e5fs around device that we've found
        let e5fs = eunix::e5fs::E5FSFilesystem::from(realpath.as_str())?;

        MountedFilesystem {
          r#type: FilesystemType::e5fs,
          driver: Box::new(e5fs),
        }
      },
      FilesystemType::binfs => {
        let binfs = BinFilesytem::new();

        MountedFilesystem {
          r#type: FilesystemType::binfs,
          driver: Box::new(binfs),
        }
      },
      FilesystemType::devfs => {
        let devfs = eunix::devfs::DeviceFilesystem::new(self.devices());

        MountedFilesystem {
          r#type: FilesystemType::devfs,
          driver: Box::new(devfs),
        }
      },
    };

    // Finally, insert constructed mounted_fs
    self.vfs.mount_points.insert(target.to_owned(), mounted_fs);

    Ok(())
  }
  pub fn umount(&mut self, target: &str) -> Result<(), Errno> {
    self.vfs.mount_points.remove(target).ok_or(Errno::ENOENT("no such mount point"))?;

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

}

// vim:ts=2 sw=2
