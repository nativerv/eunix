use std::{fmt, rc::Rc, borrow::Borrow};

use super::{fs::{Filesystem, AddressSize}, virtfs::{VirtFsFilesystem, Payload}, kernel::{Args, Kernel, Errno, Times}};

type BinaryFn = fn(Args, &mut Kernel) -> AddressSize;

#[derive(Clone)]
pub struct Binary(pub BinaryFn);

impl fmt::Debug for Binary {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      // let fun = self.0 as ;
      let fun: fn(_, &'static mut _) -> _ = self.0;
      write!(f, "{:?}", fun)
  }
}

impl fmt::Display for Binary {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      write!(f, "{:?}", self)
  }
}


fn default_binary(_: Args, _: &mut Kernel) -> AddressSize {
  0
}

impl Default for Binary {
  fn default() -> Self {
    Self(default_binary)
  }
}

pub struct BinFilesytem {
  pub virtfs: VirtFsFilesystem<Binary>,
}

impl BinFilesytem {
  pub fn new() -> Self {
    Self {
      virtfs: VirtFsFilesystem::new("binfs", 1024),
    }
  }
  pub fn write_binary(&mut self, pathname: &str, binary_fn: BinaryFn)
    -> Result<super::fs::VINode, super::kernel::Errno> {
      let vinode = self.lookup_path(pathname)?;
      self.virtfs.write_payload(&Payload::File(Binary(binary_fn)), vinode.number)?;

      Ok(vinode)
  }

  pub(crate) fn add_bins(&mut self, binary_fns: Vec<(String, BinaryFn)>) -> Result<(), Errno> {
    // Guard - check that all specified bins dont exist
    // for (pathname, result) in binary_fns
    //   .iter()
    //   .map(|(pathname, _)| { println!("{pathname}"); (pathname, self.lookup_path(pathname)) })
    // {
    //   match result {
    //     Err(Errno::ENOENT(_)) => {
    //       return Err(Errno::EEXIST(format!("binfs: add_bins: {pathname} already exists")))
    //     },
    //     Err(errno) => {
    //       return Err(Errno::EBADFS(format!("binfs: add_bins: unexpected error: ERRNO: {errno:?}")))
    //     },
    //     Ok(_) => (),
    //   }
    // }

    // Actually add binaries
    for (pathname, binary_fn) in binary_fns.iter() {
      self.create_file(pathname).expect("binfs: file creation should succeed");
      self.write_binary(pathname, *binary_fn).expect("binfs: file creation should succeed");
    }

    Ok(())
  }
  // pub fn exec_binary(&mut self, pathname: &str, kernel: &mut Kernel)
  //   -> Result<AddressSize, super::kernel::Errno> {
  //     let vinode = self.lookup_path(pathname)?;
  //
  //     let binary = match self.virtfs.read_payload(vinode.number) {
  //         Ok(Payload::File(binary)) => binary.0(),
  //         Ok(Payload::Directory(_)) => return Err(Errno::EISDIR(format!("binfs: is a directory: {pathname}"))),
  //         Err(errno) => return Err(errno),
  //     }
  //
  //     Ok(vinode)
  //   }
}

impl Filesystem for BinFilesytem {
  fn create_file(&mut self, pathname: &str)
    -> Result<super::fs::VINode, super::kernel::Errno> {
    self.virtfs.create_file(pathname)
  }

  fn remove_file(&mut self, pathname: &str)
    -> Result<(), Errno> {
    todo!()
  }

  fn create_dir(&mut self, pathname: &str)
    -> Result<super::fs::VINode, super::kernel::Errno> {
    self.virtfs.create_dir(pathname)
  }

  fn read_file(&mut self, pathname: &str, count: super::fs::AddressSize)
    -> Result<Vec<u8>, super::kernel::Errno> {
    self.virtfs.read_file(pathname, count)
  }

  fn write_file(&mut self, pathname: &str, data: &[u8])
    -> Result<super::fs::VINode, super::kernel::Errno> {
    self.virtfs.write_file(pathname, data)
  }

  fn read_dir(&mut self, pathname: &str)
    -> Result<super::fs::VDirectory, super::kernel::Errno> {
    self.virtfs.read_dir(pathname)
  }

  fn stat(&mut self, pathname: &str)
    -> Result<super::fs::FileStat, super::kernel::Errno> {
    self.virtfs.stat(pathname)
  }

  fn change_mode(&mut self, pathname: &str, mode: super::fs::FileMode)
    -> Result<(), super::kernel::Errno> {
    self.virtfs.change_mode(pathname, mode)
  }

  fn change_owners(&mut self, pathname: &str, uid: super::fs::Id, gid: super::fs::Id) 
    -> Result<(), Errno> {
    todo!()
  }

  fn change_times(&mut self, pathname: &str, times: Times)
    -> Result<(), Errno> {
    todo!()
  }

  fn lookup_path(&mut self, pathname: &str)
    -> Result<super::fs::VINode, super::kernel::Errno> {
    self.virtfs.lookup_path(pathname)
  }

  fn name(&self) -> String {
    String::from("binfs")
  }

fn as_any(&mut self) -> &mut dyn std::any::Any {
    self
  }
}

// vim:ts=2 sw=2
