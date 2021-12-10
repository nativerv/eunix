use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::io::Write;

use crate::util::any_as_u8_slice;

use super::fs::AddressSize;

pub struct DirectoryEntry<'a> {
  inode_address: AddressSize,
  name: &'a str,
  next_dir_entry_offset: AddressSize,
}

pub type Directory<'a> = Vec<DirectoryEntry<'a>>;

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

// 16 + 4 + 4 + 4 + 4 + 4 + 4 + 4 + (4 * 16) + (4 * 16)
// 16 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + (8 * 16) + (8 * 16)
pub struct Superblock {
  filesystem_type: [u8; 16],
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

pub struct Block<'a> {
  data: &'a [u8],
  next_block: AddressSize,
}

pub struct E5FSFilesystemBuilder {
  realfile: std::fs::File,
  device_size: AddressSize,
  superblock_size: AddressSize,
  inode_size: AddressSize,
  block_size: AddressSize,
  inodes_count: AddressSize,
  blocks_count: AddressSize,
  inode_table_size: AddressSize,
  filesystem_size: AddressSize,
  blocks_needed_for_free_blocks_list: AddressSize,
  first_inode_offset: AddressSize,
  first_block_offset: AddressSize,
  block_table_size: AddressSize,
}

impl E5FSFilesystemBuilder {
  pub fn new(device_realpath: &str, percent_inodes: f64, block_data_size: AddressSize) -> Self {
    let mut realfile = std::fs::OpenOptions::new()
      .write(true)
      .open(device_realpath)
      .unwrap();

    let device_size = realfile.metadata().unwrap().len() as AddressSize;
    let superblock_size = std::mem::size_of::<Superblock>() as AddressSize;
    let inode_size = std::mem::size_of::<INode>() as AddressSize;

    // block - data_pointer + data_size
    let block_size = std::mem::size_of::<Block>() as AddressSize
      - std::mem::size_of::<usize>() as AddressSize
      + block_data_size;

    let inodes_count = ((device_size as f64 * percent_inodes) / inode_size as f64) as AddressSize;
    let blocks_count =
      ((device_size as f64 * (1f64 - percent_inodes)) / block_size as f64) as AddressSize;

    let inode_table_size = inode_size * inodes_count;

    let filesystem_size = superblock_size + inode_table_size + block_size * blocks_count;

    let first_inode_offset = superblock_size;
    let first_block_offset = superblock_size + inode_table_size;

    let mut free_inodes = [0; 16];
    for i in 0..16 {
      free_inodes[i as usize] = superblock_size + (inode_size * i);
    }

    let mut free_blocks = [0; 16];
    for i in 0..16 {
      free_blocks[i as usize] = superblock_size + inode_table_size + block_size * i;
    }

    let superblock = Superblock {
      filesystem_type: [
        'e' as u8, '5' as u8, 'f' as u8, 's' as u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
      ],
      filesystem_size, // in blocks
      inode_table_size,
      free_inodes_count: inodes_count,
      free_blocks_count: blocks_count,
      inodes_count,
      blocks_count,
      block_size,
      free_inodes,
      free_blocks,
    };

    let blocks_needed_for_free_blocks_list =
      block_data_size / std::mem::size_of::<AddressSize>() as u32; // in blocks

    let block_table_size = block_size * blocks_count;

    Self {
      realfile,
      device_size,
      superblock_size,
      inode_size,

      block_size,

      inodes_count,
      blocks_count,

      inode_table_size,

      filesystem_size,
      blocks_needed_for_free_blocks_list,

      first_inode_offset,
      first_block_offset,

      block_table_size,
    }
  }
}

pub struct E5FSFilesystem {
  superblock: Superblock,
}

impl E5FSFilesystem {
  pub fn mkfs(device_realpath: &str, percent_inodes: f64, block_data_size: AddressSize) {
    let mut e5fs_builder = E5FSFilesystemBuilder::new(device_realpath, percent_inodes, block_data_size);
    let E5FSFilesystemBuilder {
      ref mut realfile,
      device_size,
      superblock_size,
      inode_size,

      block_size,

      inodes_count,
      blocks_count,

      inode_table_size,

      filesystem_size,
      blocks_needed_for_free_blocks_list,
      first_inode_offset,
      first_block_offset,

      block_table_size,
    } = e5fs_builder;

    let mut free_inodes = [0; 16];
    for i in 0..16 {
      free_inodes[i as usize] = superblock_size + (inode_size * i);
    }

    let mut free_blocks = [0; 16];
    for i in 0..16 {
      free_blocks[i as usize] = superblock_size + inode_table_size + block_size * i;
    }

    let superblock = Superblock {
      filesystem_type: [
        'e' as u8, '5' as u8, 'f' as u8, 's' as u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
      ],
      filesystem_size, // in blocks
      inode_table_size,
      free_inodes_count: inodes_count,
      free_blocks_count: blocks_count,
      inodes_count,
      blocks_count,
      block_size,
      free_inodes,
      free_blocks,
    };

    // Write Superblock
    let mut superblock_bytes = Vec::new();
    superblock_bytes.write(&superblock.filesystem_type).unwrap();
    superblock_bytes.write(&superblock.inode_table_size.to_le_bytes()) .unwrap();
    superblock_bytes.write(&superblock.free_inodes_count.to_le_bytes()) .unwrap();
    superblock_bytes.write(&superblock.inodes_count.to_le_bytes()) .unwrap();
    superblock_bytes.write(&superblock.blocks_count.to_le_bytes()) .unwrap();
    superblock_bytes.write(&superblock.block_size.to_le_bytes()) .unwrap();
    superblock_bytes.write(&superblock.free_inodes.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
    superblock_bytes.write(&superblock.free_blocks.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
    realfile.write_all(&superblock_bytes).unwrap();

    // Write blocks
    realfile.seek(SeekFrom::Start(first_block_offset.into())).unwrap();
    for address in (first_block_offset..(first_block_offset + block_table_size))
      .step_by(block_size.try_into().unwrap())
    {
      let block = Block {
        data: &vec![0; block_data_size as usize],
        next_block: 0,
      };

      E5FSFilesystem::write_block(&mut e5fs_builder, block, address);
    }

    for i in superblock_size..(superblock_size + inode_table_size) {}
  }

  fn write_block(e5fs_builder: &mut E5FSFilesystemBuilder, block: Block, address: AddressSize) {
    let mut block_bytes = Vec::new();
    block_bytes.write(&block.data).unwrap();
    block_bytes.write(&block.next_block.to_le_bytes()).unwrap();

    e5fs_builder.realfile.seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    e5fs_builder.realfile.write_all(&block_bytes).unwrap();
  }
  
  fn write_inode(e5fs_builder: &mut E5FSFilesystemBuilder, inode: INode, address: AddressSize) {
    let mut inode_bytes = Vec::new();
    inode_bytes.write(&inode.mode.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.links_count.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.uid.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.gid.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.file_size.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.atime.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.mtime.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.ctime.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.block_addresses.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).unwrap();

    e5fs_builder.realfile.seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    e5fs_builder.realfile.write_all(&inode_bytes).unwrap();
  }

  fn read_block(e5fs_builder: &mut E5FSFilesystemBuilder, address: AddressSize) -> Block {
    let mut block_bytes = Vec::new();
    block_bytes.write(&block.data).unwrap();
    block_bytes.write(&block.next_block.to_le_bytes()).unwrap();

    e5fs_builder.realfile.seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    e5fs_builder.realfile.write_all(&block_bytes).unwrap();
  }

  fn read_inode(e5fs_builder: &mut E5FSFilesystemBuilder, inode: INode, address: AddressSize) -> INode {
    use std::mem::size_of;

    let mut inode_bytes = vec![0u8; e5fs_builder.inode_size.try_into().unwrap()];

    e5fs_builder.realfile.seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    e5fs_builder.realfile.read_exact(&mut inode_bytes);

    
    // mode: u16,
    // links_count: AddressSize,
    // uid: u32,
    // gid: u32,
    // file_size: AddressSize,
    // atime: u32,
    // mtime: u32,
    // ctime: u32,
    // block_addresses: [AddressSize; 16],
    let mode = u16::from_le_bytes(inode_bytes.drain(0..size_of::<u16>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();
    let links_count = AddressSize::from_le_bytes(inode_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();
    let uid = u32::from_le_bytes(inode_bytes.drain(0..size_of::<u32>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();
    let gid = u32::from_le_bytes(inode_bytes.drain(0..size_of::<u32>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();
    let file_size = AddressSize::from_le_bytes(inode_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();
    let atime = u32::from_le_bytes(inode_bytes.drain(0..size_of::<u32>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();
    let mtime = u32::from_le_bytes(inode_bytes.drain(0..size_of::<u32>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();
    let ctime = u32::from_le_bytes(inode_bytes.drain(0..size_of::<u32>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();

    let block_addresses = Vec::new();
    for block_address in block_addresses {
      block_addresses.push(AddressSize::from_le_bytes(inode_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()));
    }


    INode {
      mode,
      links_count,
      uid,
      gid,
      file_size,
      atime,
      mtime,
      ctime,
      block_addresses: block_addresses.try_into().unwrap(),
    }
  }
}
// vim:ts=2 sw=2
