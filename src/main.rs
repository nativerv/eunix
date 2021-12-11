mod machine; 
mod eunix; 
mod util; 

use std::{path::Path, io::{SeekFrom, Read, Seek}, fs::File};
use crate::eunix::e5fs::*; 

// use machine::{Machine, OperatingSystem};

pub fn main() {
  let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("machines/1/devices/disk1.enxvd");
  let device_realpath = path.to_str().unwrap();

  let mut e5fs_builder = E5FSFilesystemBuilder::new(device_realpath, 0.05, 4096);
  println!("{:?}", device_realpath);

  // let machine = Machine::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("machines/1/machine.yaml").to_str().unwrap());
  // let os = OperatingSystem {
  //   kernel: eunix::kernel::Kernel::new(machine.get_devices()),
  // };
  //
  // println!("Machine: {:?}", machine);
  // println!();
  // println!("OS: {:?}", os);

  E5FSFilesystem::mkfs(&mut e5fs_builder);
  let root_inode_address = e5fs_builder.first_inode_address.clone(); 
  let second_to_root_inode_address = root_inode_address + e5fs_builder.inodes_count;

  let inode = INode {
    mode: 0b0000000_111_111_100,
    links_count: 2,
    uid: 1002,
    gid: 1002,
    file_size: 1488,
    atime: std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH).unwrap().as_secs().try_into().unwrap(),
    mtime: std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH).unwrap().as_secs().try_into().unwrap(),
    ctime: std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH).unwrap().as_secs().try_into().unwrap(),
    block_addresses: [3; 16],
  };
  E5FSFilesystem::write_inode(&mut e5fs_builder, inode, root_inode_address);

  let mut root_inode_bytes = vec![0u8; e5fs_builder.inode_size.try_into().unwrap()];
  e5fs_builder.realfile.seek(SeekFrom::Start(root_inode_address.try_into().unwrap())).unwrap();
  e5fs_builder.realfile.read_exact(&mut root_inode_bytes).unwrap();

  let superblock = E5FSFilesystem::read_superblock(&mut e5fs_builder);
  let root_inode = E5FSFilesystem::read_inode(&mut e5fs_builder, root_inode_address);
  let second_to_root_inode = E5FSFilesystem::read_inode(&mut e5fs_builder, second_to_root_inode_address);

  println!();
  println!("E5FSFilesystemBuilder: {:?}", e5fs_builder);
  println!();
  println!("Superblock: {:?}", superblock);
  println!();
  println!("Root INode: {:?}", root_inode);
  println!();
  println!("Second To Root INode: {:?}", second_to_root_inode);
  // println!("Root INode Bytes: {:?}", root_inode_bytes);
}

#[cfg(test)]
mod tests {
  // use super::*;

  #[test]
  fn fs_create() {

  }
}

// vim:ts=2 sw=2
