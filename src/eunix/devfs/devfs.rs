use crate::fs::Filesystem;
use crate::Kernel;

pub type Directory = Vec<E5FSDirectoryEntry>;

pub struct INode {
  mode: u16,
  links_count: AddressSize,
  uid: u32,
  gid: u32,
  file_size: AddressSize,
  atime: u32,
  mtime: u32,
  ctime: u32,
  block_addresses: [AddressSize; 16],
}

pub struct Superblock {
  type: [u8; 255],
  filesystem_size: AddressSize, // in blocks
  inode_table_size: AddressSize,
  free_inodes_count: AddressSize,
  free_blocks_count: AddressSize,
  inodes_count: AddressSize,
  blocks_count: AddressSize,
  block_size: AddressSize,
  free_inodes: [AddressSize; 16],
  free_blocks: [AddressSize; 16],
}

pub struct Block {
  is_free: bool,
  data: &[u8],
  next_block: AddressSize,
}

pub struct DeviceFilesystem {
  superblock: Superblock,
}

impl Filesystem for DeviceFilesystem {
  fn read_bytes(
    &self,
    pathname_from_fs_root: &str,
    count: AddressSize
  ) -> Result<&[u8], Error>;

  fn read_dir(pathname: &str) -> &[VDirectoryEntry];

  fn lookup_path(pathname: &str) -> VINode {

  }
}

impl DeviceFilesystem {
  fn mkfs(percent_inodes: u32, block_size: AddressSize) {

  }
}

// vim:ts=2 sw=2
