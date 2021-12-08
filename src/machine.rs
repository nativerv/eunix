use std::path::Path;

pub struct DirectoryEntry {
  inode_address: AddressSize,
  name: &str,
  next_dir_entry_offset: AddressSize,
}

pub trait VirtualDevice {
  fn get_path(&self) -> Path;
}

pub struct BlockVirtualDevice {
  path: Path,
}
impl VirtualDevice for BlockVirtualDevice {
  fn get_path(&self) {
    self.path
  }
}

pub struct OperatingSystem {
  kernel: eunix::Kernel,
}

pub struct Machine {
  devices: BTreeMap<&str, &dyn VirtualDevice>,
  os: OperatingSystem,
}

// vim:ts=2 sw=2
