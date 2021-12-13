use std::io::prelude::*;
use std::io::SeekFrom;
use std::io::Write;

use super::fs::AddressSize;
use super::fs::FileMode;
use super::fs::NO_ADDRESS;
use super::kernel::Errno;

/* 
 * LEGEND: 
 * fbl - free blocks list, the reserved blocks at the
 *       end of the blocks list which contain free
 *       block numbers for quick allocation
 * */

pub struct DirectoryEntry<'a> {
  inode_address: AddressSize,
  name: &'a str,
  next_dir_entry_offset: AddressSize,
}

pub type Directory<'a> = Vec<DirectoryEntry<'a>>;

// 2 + 4 + 4 + 4 + 4 + 4 + 4 + 4 + (4 * 16)
// 2 + 8 + 4 + 4 + 8 + 4 + 4 + 4 + (8 * 16)
#[derive(Default, Debug, PartialEq, Eq)]
pub struct INode {
  mode: FileMode,
  links_count: AddressSize,
  uid: u32,
  gid: u32,
  file_size: AddressSize,
  atime: u32,
  mtime: u32,
  ctime: u32,
  block_numbers: [AddressSize; 16],
}

// 16 + 4 + 4 + 4 + 4 + 4 + 4 + 4 + (4 * 16) + (4 * 16)
// 16 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + (8 * 16) + (8 * 16)
#[derive(Default, Debug, PartialEq)]
pub struct Superblock {
  filesystem_type: [u8; 16],
  filesystem_size: AddressSize, // in blocks
  inode_table_size: AddressSize,
  inode_table_percentage: f32,
  free_inodes_count: AddressSize,
  free_blocks_count: AddressSize,
  inodes_count: AddressSize,
  blocks_count: AddressSize,
  block_size: AddressSize,
  block_data_size: AddressSize,
  free_inode_numbers: [AddressSize; 16],
  first_fbl_block_number: AddressSize,
}

impl Superblock {
  fn new(fs_info: &mut E5FSFilesystemBuilder) -> Self {
    let _superblock_size = fs_info.superblock_size;
    let filesystem_size = fs_info.filesystem_size;
    let inode_table_size = fs_info.inode_table_size;
    let inode_table_percentage = fs_info.inode_table_percentage;
    let _inode_size = fs_info.inode_size;
    let block_size = fs_info.block_size;
    let block_data_size = fs_info.block_data_size;
    let inodes_count = fs_info.inodes_count;
    let blocks_count = fs_info.blocks_count;

    let mut free_inodes = [0; 16];
    for i in 0..16 {
      free_inodes[i as usize] = if i < inodes_count {
        i
      } else {
        NO_ADDRESS
      }
    }

    Self {
      filesystem_type: [
        'e' as u8, '5' as u8, 'f' as u8, 's' as u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
      ],
      filesystem_size, // in blocks
      inode_table_size,
      inode_table_percentage,
      free_inodes_count: inodes_count,
      free_blocks_count: blocks_count,
      inodes_count,
      blocks_count,
      block_size,
      block_data_size,
      free_inode_numbers: free_inodes,
      first_fbl_block_number: fs_info.free_blocks_count,
    }
  }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Block {
  next_block_address: AddressSize,
  data: Vec<u8>,
}

#[derive(Debug)]
#[allow(dead_code)]
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
  blocks_needed_for_fbl: AddressSize,
  first_inode_address: AddressSize,
  first_block_address: AddressSize,
  block_table_size: AddressSize,
  block_data_size: AddressSize,
  free_blocks_count: AddressSize,
  address_size: AddressSize,
  addresses_per_fbl_chunk: AddressSize,
  inode_table_percentage: f32,
  first_flb_block_number: AddressSize,
}

impl E5FSFilesystemBuilder {
  pub fn new(device_realpath: &str, inode_table_percentage: f32, block_data_size: AddressSize) -> Result<Self, &'static str> {
    // Guard for percent_inodes
    match inode_table_percentage {
      n if n < 0f32 => return Err("percent_inodes can't be less than 0"),
      n if n > 1f32 => return Err("percent_inodes can't be more than 1"),
      _ => (),
    };

    // Guard for block_data_size
    match block_data_size {
      n if (n as f64).log2().fract() != 0f64 => return Err("block_data_size must be power of 2"),
      n if n < 512 => return Err("block_data_size can't be less than 512"),
      _ => (),
    };

    let realfile = std::fs::OpenOptions::new()
      .read(true)
      .write(true)
      .open(device_realpath)
      .unwrap();

    let device_size = realfile.metadata().unwrap().len() as AddressSize;
    let superblock_size = std::mem::size_of::<Superblock>() as AddressSize;
    let inode_size = std::mem::size_of::<INode>() as AddressSize;

    let address_size = std::mem::size_of::<AddressSize>() as AddressSize;

    // next_block_number + data
    let block_size = std::mem::size_of::<AddressSize>() as AddressSize
      + block_data_size;
    

    let inodes_count = ((device_size as f32 * inode_table_percentage) / inode_size as f32) as AddressSize;
    let blocks_count =
      ((device_size as f32 * (1f32 - inode_table_percentage)) / block_size as f32) as AddressSize;

    let inode_table_size = inode_size * inodes_count;

    let filesystem_size = superblock_size + inode_table_size + block_size * blocks_count;

    let first_inode_address = superblock_size;
    let first_block_address = superblock_size + inode_table_size;

    // blocks_count / (block_data_size / block_address_size)
    let blocks_needed_for_fbl = (blocks_count as f64
      / (block_data_size as f64 / std::mem::size_of::<AddressSize>() as f64)).ceil() as AddressSize;

    let free_blocks_count = blocks_count - blocks_needed_for_fbl;

    let addresses_per_fbl_chunk = block_data_size / address_size;

    // Guard for not enough blocks even for free blocks list
    match blocks_needed_for_fbl {
    n if n >= blocks_count => return Err("disk size is too small: blocks_needed_for_fbl > blocks_count"),
      _ => (),
    }

    let block_table_size = block_size * blocks_count;

    let first_flb_block_number = free_blocks_count;

    Ok(Self {
      realfile,
      device_size,
      superblock_size,
      inode_size,

      block_size,

      inodes_count,
      blocks_count,

      inode_table_size,

      filesystem_size,
      blocks_needed_for_fbl,

      first_inode_address,
      first_block_address,

      block_table_size,
      block_data_size,

      free_blocks_count,
      address_size,
      addresses_per_fbl_chunk,
      inode_table_percentage,
      first_flb_block_number,
    })
  }
}

#[allow(dead_code)]
pub struct E5FSFilesystem {
  superblock: Superblock,
  fs_info: E5FSFilesystemBuilder,
}

impl E5FSFilesystem {
  pub fn new(device_realpath: &str, inode_table_percentage: f32, block_data_size: AddressSize) -> Result<Self, Errno> {
    let mut fs_info = 
      E5FSFilesystemBuilder::new(
        device_realpath, 
        inode_table_percentage, 
        block_data_size)
      .unwrap();

    Ok(Self {
      superblock: Superblock::new(&mut fs_info),
      fs_info,
    })
  }
  pub fn mkfs(e5fs: &mut E5FSFilesystem) -> Result<(), Errno> {
    let superblock = Superblock::new(&mut e5fs.fs_info);

    // Write Superblock
    e5fs.write_superblock(&superblock).unwrap();

    // Write fbl (free_block_list)
    e5fs.write_fbl();

    Ok(())
  }


  fn allocate_file(&mut self) -> Result<INode, Errno> {
    todo!();
  }

  // Errors:
  // ENOENT -> block_number does not exist
  fn write_block(&mut self, block: &Block, block_number: AddressSize) -> Result<(), Errno> {
    // Guard for block_number
    if block_number > self.fs_info.blocks_count {
      return Err(Errno::ENOENT)
    }

    // Read bytes from file
    let mut block_bytes = Vec::new();
    block_bytes.write(&block.next_block_address.to_le_bytes()).unwrap();
    block_bytes.write(&block.data).unwrap();

    // Get absolute address of block
    let address = self.fs_info.first_block_address + block_number * self.fs_info.block_size;

    // Seek to it and write bytes
    self.fs_info.realfile.seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    self.fs_info.realfile.write_all(&block_bytes).unwrap();

    Ok(())
  }
  
  fn write_inode(&mut self, inode: &INode, inode_number: AddressSize) -> Result<(), Errno> {
    // Guard for block_number
    if inode_number > self.fs_info.inodes_count {
      return Err(Errno::ENOENT)
    }
    
    // Read bytes from file
    let mut inode_bytes = Vec::new();
    inode_bytes.write(&inode.mode.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.links_count.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.uid.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.gid.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.file_size.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.atime.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.mtime.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.ctime.to_le_bytes()).unwrap();
    inode_bytes.write(&inode.block_numbers.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).unwrap();

    // Get absolute address of inode
    let address = self.fs_info.first_inode_address + inode_number * self.fs_info.inode_size;

    // Seek to it and write bytes
    self.fs_info.realfile.seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    self.fs_info.realfile.write_all(&inode_bytes).unwrap();

    Ok(())
  }

  fn write_superblock(&mut self, superblock: &Superblock) -> Result<(), Errno> {
    // Read bytes from file
    let mut superblock_bytes = Vec::new();
    superblock_bytes.write(&superblock.filesystem_type).unwrap();
    superblock_bytes.write(&superblock.filesystem_size.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.inode_table_size.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.inode_table_percentage.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.free_inodes_count.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.free_blocks_count.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.inodes_count.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.blocks_count.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.block_size.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.block_data_size.to_le_bytes()).unwrap();
    superblock_bytes.write(&superblock.free_inode_numbers.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
    superblock_bytes.write(&superblock.first_fbl_block_number.to_le_bytes()).unwrap();

    // Seek to 0 and write bytes
    self.fs_info.realfile.seek(SeekFrom::Start(0)).unwrap();
    self.fs_info.realfile.write_all(&superblock_bytes).unwrap();

    Ok(())
  }

  fn read_block(&mut self, block_number: AddressSize) -> Block {
    use std::mem::size_of;

    let mut block_bytes = vec![0u8; self.fs_info.block_size.try_into().unwrap()];

    // Get absolute address of block
    let address = self.fs_info.first_block_address + block_number * self.fs_info.block_size;

    // Seek to it and read bytes
    self.fs_info.realfile.seek(SeekFrom::Start(address.try_into().unwrap()).try_into().unwrap()).unwrap();
    self.fs_info.realfile.read_exact(&mut block_bytes).unwrap();

    let next_block_address = AddressSize::from_le_bytes(block_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let data = block_bytes;

    Block {
      next_block_address,
      data,
    }
  }

  fn read_inode(&mut self, inode_number: AddressSize) -> INode {
    use std::mem::size_of;

    let mut inode_bytes = vec![0u8; self.fs_info.inode_size.try_into().unwrap()];

    // Get absolute address of inode
    let address = self.fs_info.first_inode_address + inode_number * self.fs_info.inode_size;

    // Seek to it and read bytes
    self.fs_info.realfile.seek(SeekFrom::Start(address.try_into().unwrap())).unwrap();
    self.fs_info.realfile.read_exact(&mut inode_bytes).unwrap();

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
      block_numbers: block_addresses.try_into().unwrap(),
    }
  }

  fn read_superblock(&mut self) -> Superblock {
    use std::mem::size_of;

    let mut superblock_bytes = vec![0u8; self.fs_info.superblock_size.try_into().unwrap()];

    self.fs_info.realfile.seek(SeekFrom::Start(0)).unwrap();
    self.fs_info.realfile.read_exact(&mut superblock_bytes).unwrap();

    let filesystem_type: [u8; 16] = superblock_bytes.drain(0..16).as_slice().try_into().unwrap(); 
    let filesystem_size = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let inode_table_size = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let inode_table_percentage = f32::from_le_bytes(superblock_bytes.drain(0..size_of::<f32>()).as_slice().try_into().unwrap());
    let free_inodes_count = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let free_blocks_count = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let inodes_count = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let blocks_count = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let block_size = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());
    let block_data_size = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());

    let mut free_inode_numbers = Vec::new();
    for _ in 0..16 {
      free_inode_numbers.push(AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap()));
    }

    let first_fbl_block_number = AddressSize::from_le_bytes(superblock_bytes.drain(0..size_of::<AddressSize>()).as_slice().try_into().unwrap());

    Superblock {
      filesystem_type,
      filesystem_size,
      inode_table_size,
      inode_table_percentage,
      free_inodes_count,
      free_blocks_count,
      inodes_count,
      blocks_count,
      block_size,
      block_data_size,
      free_inode_numbers: free_inode_numbers.try_into().unwrap(),
      first_fbl_block_number,
    }
  }

  /// Parse one fbl block and return it for further use
  fn read_block_numbers_from_block(block: &Block) -> Vec<AddressSize> {
    use std::mem::size_of;
    let data = block.data.clone();

    data
      .chunks(size_of::<AddressSize>())
      .map(|chunk| AddressSize::from_le_bytes(chunk.try_into().unwrap()))
      .collect::<Vec<AddressSize>>()
  }

  fn generate_fbl(&self) -> Vec<Vec<AddressSize>> {
    // Basically what we do in the next ~30 lines:
    // 1. Generate addresses of free blocks
    // 2. Divide it to chunks of however many fits to one block
    // 3. If addresses doesnt divide into chunks evenly,
    //    leave a remainder in separate small chunk
    // 4. If above is the case and remainder was created,
    //    fill it to be the size of full chunks
    //    with NO_BLOCK addresses
    let free_blocks_numbers = 
      (0..self.fs_info.first_flb_block_number).collect::<Vec<AddressSize>>();

    let fbl_chunks_iterator = 
      free_blocks_numbers
        .chunks_exact(self.fs_info.addresses_per_fbl_chunk as usize);

    let mut fbl_chunks = 
      fbl_chunks_iterator
        .clone()
        .map(|chunk| chunk.to_owned())
        .collect::<Vec<Vec<AddressSize>>>();

    let mut remainder = fbl_chunks_iterator
      .remainder()
      .to_vec();

    let lacking_addresses_count = self.fs_info.addresses_per_fbl_chunk - remainder.len() as AddressSize;

    if lacking_addresses_count > 0 {
      remainder
        .append(&mut vec![NO_ADDRESS; lacking_addresses_count as usize]);
    }

    fbl_chunks.push(remainder);

    fbl_chunks
  }

  fn write_fbl(&mut self) {
    let fbl_chunks = self.generate_fbl();

    // Write free blocks list to last N blocks
    // [ sb ... i1..iN ... b1[b1..bX ... fbl1..fblN]bN ]
    // Something like that ^
    // fbl - fbl
    fbl_chunks
      .iter()
    // Zip chunks with fbl block numbers
      .zip(self.fs_info.first_flb_block_number..self.fs_info.blocks_count)
      .for_each(|(chunk, block_number)| {
        let block = Block {
          next_block_address: if block_number != self.fs_info.blocks_count - 1 {
            block_number + 1
          } else {
            NO_ADDRESS
          },
          data: chunk.iter().flat_map(|x| x.to_le_bytes()).collect(),
        };
        self.write_block(&block, block_number).unwrap();
      });
  }
}

#[cfg(test)]
mod tests {
  use crate::util::{mktemp, mkenxvd};
  use super::*;

  #[test]
  fn write_superblock_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::new(tempfile.as_str(), 0.05, 4096).unwrap();

    let superblock = Superblock::new(&mut e5fs.fs_info);

    e5fs.write_superblock(&superblock).unwrap();

    let superblock_from_file = e5fs.read_superblock();

    assert_eq!(superblock_from_file, superblock);
  }

  #[test]
  fn write_inode_works() {
    // let tempfile = "/tmp/tmp.4yOs4cciU1".to_owned();
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::new(tempfile.as_str(), 0.05, 4096).unwrap(); 

    let inode_indices = 0..e5fs.fs_info.blocks_count;
    let inodes = inode_indices.clone().fold(Vec::new(), |mut vec, i| {
      vec.push(INode {
        mode: 0b0000000_000_000_000 + 1,
        links_count: i,
        uid: i,
        gid: i + 1,
        file_size: i * 1024,
        atime: crate::util::unixtime(),
        mtime: crate::util::unixtime(),
        ctime: crate::util::unixtime(),
        block_numbers: [i % 5; 16],
      });

      vec
    });

    inodes
      .iter()
      .zip(inode_indices.clone())
      .for_each(|(inode, inode_number)| {
        e5fs.write_inode(inode, inode_number).unwrap();
      });

    inodes
      .iter()
      .zip(inode_indices.clone())
      .for_each(|(inode, inode_number)| {
        let inode_from_file = e5fs.read_inode(inode_number);

        assert_eq!(*inode, inode_from_file);
      });
  }

  #[test]
  fn write_block_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::new(tempfile.as_str(), 0.05, 4096).unwrap();

    let block_indices = 0..e5fs.fs_info.blocks_count;
    let blocks = block_indices.clone().fold(Vec::new(), |mut vec, i| {
      vec.push(Block {
        next_block_address: i,
        data: vec![(i % 255) as u8; e5fs.fs_info.block_data_size as usize],
      });

      vec
    });

    blocks
      .iter()
      .zip(block_indices.clone())
      .for_each(|(block, block_number)| {
        e5fs.write_block(block, block_number).unwrap();
      });

    blocks
      .iter()
      .zip(block_indices.clone())
      .for_each(|(block, block_number)| {
        let block_from_file = e5fs.read_block(block_number);

        assert_eq!(*block, block_from_file);
      });
  }

  #[test]
  fn write_fbl_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let mut e5fs = E5FSFilesystem::new(tempfile.as_str(), 0.05, 4096).unwrap();

    e5fs.write_fbl();

    let fbl_chunks = e5fs.generate_fbl();

    let fbl_chunks_from_file = (e5fs.fs_info.first_flb_block_number..e5fs.fs_info.blocks_count)
      .map(|block_number| {
        let block = e5fs.read_block(block_number);
        let block_numbers_from_block = E5FSFilesystem::read_block_numbers_from_block(&block);

        block_numbers_from_block
      })
      .collect::<Vec<Vec<AddressSize>>>();

    println!("free_blocks_count: {}", e5fs.fs_info.free_blocks_count);
    println!("blocks_count: {}", e5fs.fs_info.blocks_count);
    println!("blocks_needed_for_fbl: {}", e5fs.fs_info.blocks_needed_for_fbl);
    assert_eq!(fbl_chunks_from_file, fbl_chunks);
  }

  #[test]
  fn read_block_numbers_from_block_works() {
    let tempfile = mktemp().to_owned();
    mkenxvd("1M".to_owned(), tempfile.clone());

    let e5fs = E5FSFilesystem::new(tempfile.as_str(), 0.05, 4096).unwrap();

    let block_numbers_per_free_blocks_chunk = e5fs.fs_info.block_data_size / std::mem::size_of::<AddressSize>() as AddressSize;

    let block_numbers: Vec<AddressSize> = (0..block_numbers_per_free_blocks_chunk).collect();

    let block = Block {
      next_block_address: NO_ADDRESS,
      data: block_numbers.iter().flat_map(|x| x.to_le_bytes()).collect(),
    };

    let block_numbers_from_block = E5FSFilesystem::read_block_numbers_from_block(&block);

    assert_eq!(block_numbers_from_block, block_numbers);
  }
}

// vim:ts=2 sw=2
