use std::io::prelude::*;
use std::io::SeekFrom;
use std::io::Write;

use super::fs::AddressSize;

pub struct DirectoryEntry<'a> {
  inode_address: AddressSize,
  name: &'a str,
  next_dir_entry_offset: AddressSize,
}

pub type Directory<'a> = Vec<DirectoryEntry<'a>>;

#[derive(Default, Debug)]
pub struct INode {
  pub mode: u16,
  pub links_count: AddressSize,
  pub uid: u32,
  pub gid: u32,
  pub file_size: AddressSize,
  pub atime: u32,
  pub mtime: u32,
  pub ctime: u32,
  pub block_addresses: [AddressSize; 16],
}

// 16 + 4 + 4 + 4 + 4 + 4 + 4 + 4 + (4 * 16) + (4 * 16)
// 16 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + (8 * 16) + (8 * 16)
#[derive(Default, Debug)]
pub struct Superblock {
  filesystem_type: [u8; 16],
  filesystem_size: AddressSize, // in blocks
  inode_table_size: AddressSize,
  free_inodes_count: AddressSize,
  free_blocks_count: AddressSize,
  inodes_count: AddressSize,
  blocks_count: AddressSize,
  block_size: AddressSize,
  free_inode_addresses: [AddressSize; 16],
  free_block_addresses: [AddressSize; 16],
}

#[derive(Default, Debug)]
pub struct Block {
  next_block_address: AddressSize,
  data: Vec<u8>,
}

#[derive(Debug)]
pub struct E5FSFilesystemBuilder {
  pub realfile: std::fs::File,
  pub device_size: AddressSize,
  pub superblock_size: AddressSize,
  pub inode_size: AddressSize,
  pub block_size: AddressSize,
  pub inodes_count: AddressSize,
  pub blocks_count: AddressSize,
  pub inode_table_size: AddressSize,
  pub filesystem_size: AddressSize,
  pub blocks_needed_for_free_blocks_list: AddressSize,
  pub first_inode_address: AddressSize,
  pub first_block_address: AddressSize,
  pub block_table_size: AddressSize,
}

impl E5FSFilesystemBuilder {
  pub fn new(device_realpath: &str, percent_inodes: f64, block_data_size: AddressSize) -> Self {
    let realfile = std::fs::OpenOptions::new()
      .read(true)
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

    let first_inode_address = superblock_size;
    let first_block_address = superblock_size + inode_table_size;

    let mut free_inodes = [0; 16];
    for i in 0..16 {
      free_inodes[i as usize] = superblock_size + (inode_size * i);
    }

    let mut free_blocks = [0; 16];
    for i in 0..16 {
      free_blocks[i as usize] = superblock_size + inode_table_size + block_size * i;
    }

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

      first_inode_address,
      first_block_address,

      block_table_size,
    }
  }
}

pub struct E5FSFilesystem {
  superblock: Superblock,
}

impl E5FSFilesystem {
  pub fn mkfs(e5fs_builder: &mut E5FSFilesystemBuilder) {
    let _realfile = &e5fs_builder.realfile;
    let _device_size = e5fs_builder.device_size;
    let superblock_size = e5fs_builder.superblock_size;
    let inode_size = e5fs_builder.inode_size;

    let block_size = e5fs_builder.block_size;

    let inodes_count = e5fs_builder.inodes_count;
    let blocks_count = e5fs_builder.blocks_count;

    let inode_table_size = e5fs_builder.inode_table_size;
    let block_table_size = e5fs_builder.block_table_size;

    let filesystem_size = e5fs_builder.filesystem_size;
    let _blocks_needed_for_free_blocks_list = e5fs_builder.blocks_needed_for_free_blocks_list;
    let first_inode_address = e5fs_builder.first_inode_address;
    let first_block_address = e5fs_builder.first_block_address;

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
      free_inode_addresses: free_inodes,
      free_block_addresses: free_blocks,
    };

    // Write Superblock
    E5FSFilesystem::write_superblock(e5fs_builder, superblock);

    // Write blocks
    for address in (first_block_address..(first_block_address + block_table_size))
      .step_by(block_size.try_into().unwrap())
    {
      // let block = Block {
      //   data: vec![0; block_data_size as usize],
      //   next_block_address: 0,
      // };

      E5FSFilesystem::write_block(e5fs_builder, Block::default(), address);
    }

    // Write inodes
    for address in (first_inode_address..first_block_address) 
      .step_by(block_size.try_into().unwrap())
    {
      // let mut inode = INode::default();
      // inode.links_count = 1;
      let inode = INode {
        mode: 0b0000000_111_111_100,
        links_count: 2,
        uid: 1000,
        gid: 1000,
        file_size: 228,
        atime: 1337,
        mtime: 1337,
        ctime: 1337,
        block_addresses: [3; 16],
      };
      // println!("INode: {:?}", inode);

      E5FSFilesystem::write_inode(e5fs_builder, inode, address);
    }
  }

  fn write_block(e5fs_builder: &mut E5FSFilesystemBuilder, block: Block, address: AddressSize) {
    let mut block_bytes = Vec::new();
    block_bytes.write(&block.data).unwrap();
    block_bytes.write(&block.next_block_address.to_le_bytes()).unwrap();

    e5fs_builder.realfile.seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    e5fs_builder.realfile.write_all(&block_bytes).unwrap();
  }
  
  pub fn write_inode(e5fs_builder: &mut E5FSFilesystemBuilder, inode: INode, address: AddressSize) {
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

  fn write_superblock(e5fs_builder: &mut E5FSFilesystemBuilder, superblock: Superblock) {
    // pub struct Superblock {
    //   filesystem_type: [u8; 16],
    //   filesystem_size: AddressSize, // in blocks
    //   inode_table_size: AddressSize,
    //   free_inodes_count: AddressSize,
    //   free_blocks_count: AddressSize,
    //   inodes_count: AddressSize,
    //   blocks_count: AddressSize,
    //   block_size: AddressSize,
    //   free_inodes: [AddressSize; 16],
    //   free_blocks: [AddressSize; 16],
    // }

    let mut superblock_bytes = Vec::new();
    superblock_bytes.write(&superblock.filesystem_type).unwrap();
    superblock_bytes.write(&superblock.filesystem_size.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.inode_table_size.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.free_inodes_count.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.free_blocks_count.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.inodes_count.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.blocks_count.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.block_size.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.free_inode_addresses.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
    superblock_bytes.write(&superblock.free_block_addresses.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).unwrap();

    e5fs_builder.realfile.seek(SeekFrom::Start(0)).unwrap();
    e5fs_builder.realfile.write_all(&superblock_bytes).unwrap();
  }

  fn read_block(e5fs_builder: &mut E5FSFilesystemBuilder, address: AddressSize) -> Block {
    use std::mem::size_of;

    let mut inode_bytes = vec![0u8; e5fs_builder.superblock_size.try_into().unwrap()];

    e5fs_builder.realfile.seek(SeekFrom::Start(address.try_into().unwrap()).try_into().unwrap()).unwrap();
    e5fs_builder.realfile.read_exact(&mut inode_bytes).unwrap();

    let next_block_address = AddressSize::from_le_bytes(inode_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();
    let data = inode_bytes;

    Block {
      next_block_address,
      data,
    }
  }

  pub fn read_inode(e5fs_builder: &mut E5FSFilesystemBuilder, address: AddressSize) -> INode {
    use std::mem::size_of;

    let mut inode_bytes = vec![0u8; e5fs_builder.inode_size.try_into().unwrap()];

    e5fs_builder.realfile.seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    e5fs_builder.realfile.read_exact(&mut inode_bytes).unwrap();

    let mode = u16::from_le_bytes(inode_bytes.drain(0..size_of::<u16>()).as_slice().try_into().unwrap()); 
    let links_count = AddressSize::from_le_bytes(inode_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()); 
    let uid = u32::from_le_bytes(inode_bytes.drain(0..size_of::<u32>()).as_slice().try_into().unwrap()); 
    let gid = u32::from_le_bytes(inode_bytes.drain(0..size_of::<u32>()).as_slice().try_into().unwrap());
    let file_size = AddressSize::from_le_bytes(inode_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let atime = u32::from_le_bytes(inode_bytes.drain(0..size_of::<u32>()).as_slice().try_into().unwrap());
    let mtime = u32::from_le_bytes(inode_bytes.drain(0..size_of::<u32>()).as_slice().try_into().unwrap());
    let ctime = u32::from_le_bytes(inode_bytes.drain(0..size_of::<u32>()).as_slice().try_into().unwrap());

    let mut block_addresses = Vec::new();
    for _ in 0..16 {
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

  pub fn read_superblock(e5fs_builder: &mut E5FSFilesystemBuilder) -> Superblock {
    use std::mem::size_of;

    let mut superblock_bytes = vec![0u8; e5fs_builder.superblock_size.try_into().unwrap()];

    e5fs_builder.realfile.seek(SeekFrom::Start(0)).unwrap();
    e5fs_builder.realfile.read_exact(&mut superblock_bytes).unwrap();

    
    // pub struct Superblock {
    //   filesystem_type: [u8; 16],
    //   filesystem_size: AddressSize, // in blocks
    //   inode_table_size: AddressSize,
    //   free_inodes_count: AddressSize,
    //   free_blocks_count: AddressSize,
    //   inodes_count: AddressSize,
    //   blocks_count: AddressSize,
    //   block_size: AddressSize,
    //   free_inodes: [AddressSize; 16],
    //   free_blocks: [AddressSize; 16],
    // }
    let filesystem_type: [u8; 16] = superblock_bytes.drain(0..16).as_slice().try_into().unwrap(); 
    let filesystem_size = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();
    let inode_table_size = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();
    let free_inodes_count = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();
    let free_blocks_count = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();
    let inodes_count = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();
    let blocks_count = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();
    let block_size = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()); // to_le_bytes()).unwrap();;

    let mut free_inode_addresses = Vec::new();
    for _ in 0..16 {
      free_inode_addresses.push(AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()));
    }

    let mut free_block_addresses = Vec::new();
    for _ in 0..16 {
      free_block_addresses.push(AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()));
    }


    Superblock {
      filesystem_type,
      filesystem_size,
      inode_table_size,
      free_inodes_count,
      free_blocks_count,
      inodes_count,
      blocks_count,
      block_size,
      free_inode_addresses: free_inode_addresses.try_into().unwrap(),
      free_block_addresses: free_block_addresses.try_into().unwrap(),
    }
  }
}

// vim:ts=2 sw=2
