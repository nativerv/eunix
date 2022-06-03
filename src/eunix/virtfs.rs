use std::any::Any;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::fmt;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Display;
use std::slice::SliceIndex;

// use fancy_regex::Regex;

use crate::eunix::fs::FileModeType;
use crate::eunix::fs::NOBODY_UID;
use crate::eunix::kernel::KERNEL_MESSAGE_HEADER_ERR;
// use crate::util::fixedpoint;
// use crate::util::unixtime;

use super::fs::AddressSize;
use super::fs::FileMode;
use super::fs::FileStat;
use super::fs::Filesystem;
use super::fs::Id;
use super::fs::NO_ADDRESS;
use super::fs::VDirectory;
use super::fs::VDirectoryEntry;
use super::fs::VINode;
use super::fs::VFS;
use super::kernel::Errno;
use super::kernel::Times;
use super::kernel::UnixtimeSize;

pub trait VirtFsFile = Clone + Default + fmt::Display;

const ROOT_INODE_NUMBER: AddressSize = 0;

/* 
 * LEGEND: 
 * fbl       - free blocks list, the reserved blocks at the
 *             end of the blocks list which contain free
 *             block numbers for quick allocation
 * fbl_chunk - vector of numbers parsed from fbl block
 * */

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
  pub inode_number: AddressSize,
  pub name: String,
}

impl DirectoryEntry {
  fn new(inode_number: AddressSize, name: &str) -> Result<Self, Errno> {
    Ok(Self {
      inode_number,
      name: name.to_owned(),
    })
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directory {
  pub entries: BTreeMap<String, DirectoryEntry>,
}

impl Directory {
  pub fn new() -> Self {
    Self {
      entries: BTreeMap::new(),
    }
  }

  fn from(entries: BTreeMap<String, DirectoryEntry>) -> Self {
    Self {
      entries,
    }
  }

  // pub fn entries_count(&self) -> AddressSize {
  //   self.entries_count
  // }
  // pub fn entries(&self) -> BTreeMap<String, DirectoryEntry> {
  //   self.entries
  // }

  pub fn insert(&mut self, inode_number: AddressSize, name: &str) -> Result<(), Errno> {
    self.entries.insert(
      name.to_owned(),
      DirectoryEntry::new(inode_number, name)?
    );
    Ok(())
  }
  pub fn remove(&mut self, name: &str) -> Result<(), Errno> {
    self.entries.remove(name).ok_or(Errno::ENOENT(String::from("no such file in directory")))?;
    Ok(())
  }
}

#[derive(Debug, Clone)]
pub enum Payload<T: VirtFsFile> {
  Directory(Directory),
  File(T),
}

impl<T: VirtFsFile> fmt::Display for Payload<T> {
  fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
    match self {
      Payload::Directory(dir) => write!(formatter, "{:?}", dir),
      Payload::File(file) => write!(formatter, "{}", file),
    }
  }
}

impl<T: VirtFsFile> Default for Payload<T> {
  fn default() -> Self {
    Payload::File(T::default())
  }
}

#[derive(Debug, Clone)]
pub struct INode {
  mode: FileMode,
  links_count: AddressSize,
  uid: Id,
  gid: Id,
  file_size: AddressSize,
  atime: UnixtimeSize,
  mtime: UnixtimeSize,
  ctime: UnixtimeSize,
  btime: UnixtimeSize,
  payload_number: AddressSize,
  number: AddressSize,
}

impl fmt::Display for INode {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self)
  }
}

impl Default for INode {
  fn default() -> Self {
    Self {
      mode: FileMode::default(),
      links_count: 0,
      file_size: 0,
      uid: NOBODY_UID,
      gid: NOBODY_UID,
      atime: 0,
      mtime: 0,
      ctime: 0,
      btime: 0,
      payload_number: NO_ADDRESS,
      number: 0,
    }
  }
}

impl From<INode> for VINode {
  fn from(inode: INode) -> Self {
    Self {
      mode: inode.mode,
      links_count: inode.links_count,
      file_size: inode.file_size,
      uid: inode.uid,
      gid: inode.gid,
      atime: inode.atime,
      ctime: inode.ctime,
      mtime: inode.mtime,
      btime: inode.btime,
      number: inode.number,
    }
  }
}

impl From<DirectoryEntry> for VDirectoryEntry {
  fn from(entry: DirectoryEntry) -> Self {
    Self {
      inode_number: entry.inode_number,
      name: entry.name,
    }
  }
}

impl From<Directory> for VDirectory {
  fn from(dir: Directory) -> Self {
    Self {
      entries: dir.entries.into_iter().map(|(key, entry)| (key, entry.into())).collect(),
    }
  }
}

// 16 + 4 + 4 + 4 + 4 + 4 + 4 + 4 + (4 * 16) + (4 * 16)
// 16 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + (8 * 16) + (8 * 16)
#[derive(Default, Debug)]
pub struct Superblock {
  pub filesystem_size: AddressSize, // in blocks
  pub blocks_count: AddressSize,
}


impl Superblock {
  fn new() -> Self {
    Self {
      filesystem_size: 0,
      blocks_count: 0,
    }
  }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Block {
  data: Vec<u8>,
}

pub struct VirtFsFilesystem<T: VirtFsFile> {
  pub superblock: Superblock,
  pub name: String,
  pub inodes: Vec<INode>,
  pub payloads: Vec<Option<Payload<T>>>,
}

impl<T: VirtFsFile> VirtFsFilesystem<T> {
  /// Returns:
  /// ENOENT -> if no free block or inode exists
  fn allocate_file(&mut self) -> Result<AddressSize, Errno> {
    let inode_number = self.claim_free_inode()?;

    let mut inode = INode {
      mode: FileMode::default().with_free(0),
      links_count: 0,
      file_size: 0,
      uid: NOBODY_UID,
      gid: NOBODY_UID,
      atime: 0,
      mtime: 0,
      ctime: 0,
      number: inode_number,
      ..Default::default()
    };

    let free_payload_number = self.claim_free_payload()?;
    inode.payload_number = free_payload_number;

    self.write_inode(&inode, inode_number)?;
    self.write_payload(&Payload::default(), inode_number)?;

    Ok(inode_number)
  }

  fn claim_free_inode(&mut self) -> Result<AddressSize, Errno> {
    if let Some(inode_number) = self
      .inodes
      .iter()
      .position(|inode| inode.mode.free() == 1)
    {
      let inode = self
        .inodes
        .get_mut(inode_number)
        .expect("virtfs: we know that inode_number exists");
      inode.mode = inode.mode.with_free(0);
      Ok(inode_number as AddressSize)
    } else {
      Err(Errno::ENOSPC(String::from("virtfs: no free inodes left")))
    }
  }

  fn claim_free_payload(&self) -> Result<AddressSize, Errno> {
    if let Some(payload_number) = self
      .payloads
      .iter()
      .position(Option::is_none)
    {
        Ok(payload_number as AddressSize)
    } else {
      // self.payloads.push(None);
      // Ok(self.payloads.len() as AddressSize - 1)
      Err(Errno::ENOSPC(String::from("virtfs: no free blocks (payloads) left")))
    }
  }

  fn write_inode(&mut self, inode: &INode, free_inode_number: u32) -> Result<(), Errno> {
    *self
      .inodes
      .get_mut(free_inode_number as usize)
      .ok_or(
        Errno::EIO(String::from("virtfs: write_inode: no such inode"))
      )? = inode.clone();

    Ok(())
  }
  pub fn write_payload(&mut self, payload: &Payload<T>, free_payload_number: u32) -> Result<(), Errno> {
    *self
      .payloads
      .get_mut(free_payload_number as usize)
      .ok_or(
        Errno::EIO(String::from("virtfs: write_payload: no such inode"))
      )? = Some(payload.clone());

    Ok(())
  }
}

impl<T: VirtFsFile> Filesystem for VirtFsFilesystem<T> {
  fn create_file(&mut self, pathname: &str)
    -> Result<VINode, Errno> {
    // Regex matching final_component of path (+ leading slash)
    let (everything_else, dirent_name) = VFS::split_path(pathname)?;
    // NOTICE: there may be problems/conflicts with leading / insertion
    //         in intermediate path representations
    //         see: `Filesystem::lookup_path` implementations,
    //              `VFS::match_mount_point`
    let dir_pathname = format!("/{}", everything_else.join("/"));

    // Get dir path with this regex
    let dir_inode = self.lookup_path(dir_pathname.as_str())?;

    // Read dir from disk
    let mut dir = self.read_dir_from_inode(dir_inode.number)?;

    // Guard for file already
    if let Some(_) = dir.entries
      .iter()
      .find(|(name, _entry)| format!("/{}", name) == dirent_name)
    {
       return Err(Errno::EINVAL(String::from("file already exists")));
    }

    // Allocate inode
    let file_inode_number = self.allocate_file()?;

    // Push allocated to dir
    dir.insert(file_inode_number, dirent_name.as_str())?;

    // Write dir
    self.write_dir(&dir, dir_inode.number)?;

    let file_inode = self.read_inode(file_inode_number)?;

    Ok(file_inode.into())
  }

  fn remove_file(&mut self, pathname: &str)
    -> Result<(), Errno> {
        todo!()
    } 

  fn create_dir(&mut self, pathname: &str)
    -> Result<VINode, Errno> {
    let vinode = self.create_file(pathname)?;
    self.change_mode(pathname, vinode.mode.with_file_type(FileModeType::Dir as u8))?;
    self.write_payload(&Payload::Directory(Directory::new()), vinode.number)?;

    Ok(vinode)
  }

  fn read_file(&mut self, pathname: &str, _count: AddressSize)
    -> Result<Vec<u8>, Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    let file = self
      .read_from_file(inode_number)?;

    Ok(
      format!("{file}")
        .as_bytes()
        .to_owned()
    )
  } 

  fn write_file(&mut self, pathname: &str, data: &[u8])
    -> Result<VINode, Errno> {
      todo!("Accept callbacks for read and write from the instantiator")
  }

  fn read_dir(&self, pathname: &str)
    -> Result<VDirectory, Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    let dir = self.read_dir_from_inode(inode_number)?;

    Ok(dir.into())
  }

  fn stat(&self, pathname: &str) 
    -> Result<FileStat, Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    let INode {
      mode,
      file_size,
      links_count,
      uid,
      gid,
      atime,
      mtime,
      ctime,
      btime,
      ..
    } = self.read_inode(inode_number)?;

    Ok(FileStat {
      mode,
      size: file_size,
      inode_number,
      links_count,
      uid,
      gid,
      block_size: 0,
      atime,
      mtime,
      ctime,
      btime,
    })
  }

  fn change_mode(&mut self, pathname: &str, mode: FileMode)
    -> Result<(), Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    self.write_mode(inode_number, mode)
  } 

  fn change_times(&mut self, pathname: &str, times: Times)
    -> Result<(), Errno> {
    todo!()
  }

  // Поиск файла в файловой системе. Возвращает INode фала.
  // Для VFS сначала матчинг на маунт-поинты и вызов lookup_path("/mount/point") у конкретной файловой системы;
  // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
  fn lookup_path(&self, pathname: &str)
    -> Result<VINode, Errno> {
    let pathname = VFS::split_path(pathname)?;
    let (everything_else, final_component) = pathname.clone();
    let inode: INode = self.read_inode(ROOT_INODE_NUMBER)?;

    // Base case
    if pathname == (Vec::new(), String::from("/")) {
      let inode = self.read_inode(ROOT_INODE_NUMBER)?;
      return Ok(inode.into());
    };

    fn is_dir(inode: VINode) -> bool {
      // TODO: critical bug: inode mode and... nevermind, the present is correct
      let filetype = inode.mode.file_type();
      filetype == FileModeType::Dir as u8
    }

    // TODO: add 'blocks' vector to the VirtFsFilesystem: Vec<T>, indexed by inodes with payload_index
    fn find_dir<T: VirtFsFile>(virtfs: &VirtFsFilesystem<T>, everything_else: Vec<String>, initial_inode: &INode) -> Result<INode, Errno> {
      let mut inode = initial_inode.clone();

      let mut everything_else = VecDeque::from(everything_else);
      // TODO: pass inode to read_dir_from_inode
      while everything_else.len() > 0 {
        if !is_dir(inode.clone().into()) {
          return Err(Errno::ENOTDIR(String::from("virtfs.lookup_path: not a directory (find_dir)")))
        }

        let piece = everything_else.pop_front().unwrap();
        let dir = virtfs.read_dir_from_inode(inode.number)?;
        if let Some(entry) = dir.entries.get(&piece.to_owned()) {
          inode = virtfs.read_inode(entry.inode_number)?;
        } else {
          return Err(Errno::ENOENT(String::from("virtfs.lookup_path: no such file or directory")))
        }
      }

      Ok(inode)
    }

    // Try to find directory - "everything else" part of `pathname`
    let dir_inode = find_dir(self, everything_else, &inode)?;
    let dir = self.read_dir_from_inode(dir_inode.number)?;

    // Try to find file in directory and map its INode to VINode -
    // "final component" part of `pathname`, then return it
    Ok(
      dir.entries
        .get(&final_component)
        .ok_or_else(|| Errno::ENOENT(String::from("virtfs.lookup_path: no such file or directory (get(final_component))")))
        // Read its inode_number
        .and_then(|entry| self.read_inode(entry.inode_number))?
        .into()
    )
  }

  fn name(&self) -> String { 
    self.name().clone()
  }

  fn as_any(&mut self) -> &mut dyn Any {
    unimplemented!("virtfs: as_any for virtfs is undefined")
  } 
}

impl<T: VirtFsFile> VirtFsFilesystem<T> {
  /// Construct new virtfs
  pub fn new(name: &str, inodes_count: AddressSize) -> Self {
    let mut virtfs = Self {
      superblock: Superblock::new(),
      name: name.to_owned(),
      inodes: vec![Default::default(); inodes_count as usize],
      payloads: vec![None; inodes_count as usize],
    };

    // Create the root inode
    let root_payload_number = virtfs.claim_free_payload().expect("virtfs: this must succeed");
    let mut root_inode = virtfs.read_inode(ROOT_INODE_NUMBER).expect("virtfs: this must succeed");
    root_inode.mode = root_inode
      .mode
      .with_free(0)
      .with_file_type(FileModeType::Dir as u8)
    ;
    root_inode.payload_number = root_payload_number;

    // Create root directory
    let mut dir = Directory::new();
    dir.insert(root_inode.number, "..").unwrap();
    dir.insert(root_inode.number, ".").unwrap();

    // Write root inode
    virtfs.write_inode(&root_inode, ROOT_INODE_NUMBER).expect("virtfs: this must succeed");
    virtfs.write_dir(&dir, ROOT_INODE_NUMBER).expect("virtfs: this must succeed");

    virtfs
  }

  pub fn name(&self) -> String {
    self.name.clone()
  }

  fn write_dir(&mut self, dir: &Directory, inode_number: AddressSize) -> Result<(), Errno> {
    if let Some(inode) = self.inodes.get_mut(inode_number as usize) {
      // Ебобо совсем?
      // let payload_number = self
      //   .inodes
      //   .get(inode_number as usize)
      //   .ok_or(Errno::EIO(String::from("virtfs: inode does not exist for inode_number")))?
      //   .payload_number
      // ;

      let payload_number = inode.payload_number;

      *self.payloads.get_mut(payload_number as usize)
        .ok_or(Errno::EIO(String::from("virtfs: payload does not exist for payload_number")))?
        = Some(Payload::Directory(dir.clone()));

      Ok(())
    } else {
      Err(Errno::ENOENT(String::from("virtfs: no such file or directory")))
    }
  }

  fn read_dir_from_inode(&self, inode_number: AddressSize) -> Result<Directory, Errno> {
    match self.read_from_file(inode_number)? {
      Payload::Directory(directory) => Ok(directory),
      Payload::File(_) => Err(Errno::ENOTDIR(String::from("tried to read file from inode (TODO: inode number here), got directory"))),
    }
  }

  fn read_from_file(&self, inode_number: AddressSize) -> Result<Payload<T>, Errno> {
    let payload_number = self.read_inode(inode_number)?.payload_number;

    let payload = self
      .payloads
      .get(payload_number as usize)
      .to_owned()
      .ok_or(Errno::EIO(format!("virtfs: read_from_file: no payload for inode #{inode_number} (payload_number was #{payload_number}) [1]")))?
      .to_owned()
      .ok_or(Errno::EIO(format!("virtfs: read_from_file: no payload for inode #{inode_number} (payload_number was #{payload_number}) [2]")))?
    ;

    Ok(payload)
  }

  fn get_inode_blocks_count(&mut self, inode_number: AddressSize) -> Result<AddressSize, Errno> {
    Ok(0)
  }

  fn read_mode(&mut self, inode_number: AddressSize) -> Result<FileMode, Errno> {
    let inode = self.read_inode(inode_number)?;
    Ok(inode.mode)
  }

  fn write_mode(&mut self, inode_number: AddressSize, mode: FileMode) -> Result<(), Errno> {
    let mut inode = self.read_inode(inode_number)?;
    inode.mode = mode;
    self.write_inode(&inode, inode_number)?;
    Ok(())
  }

  fn read_inode(&self, inode_number: AddressSize) -> Result<INode, Errno> {
    Ok(
      self
       .inodes
       .get(inode_number as usize)
       .ok_or(Errno::ENOENT(String::from("virtfs: read_inode: no such file or directory")))?
       .clone()
     )
  }
  pub fn read_payload(&mut self, inode_number: AddressSize) -> Result<Payload<T>, Errno> {
    Ok(
      self
       .inodes
       .get(inode_number as usize)
       .and_then(|inode| {
          self
           .payloads
           .get(inode.payload_number as usize)
       })
       .ok_or(Errno::ENOENT(format!("virtfs: read_payload: payload does not exist for inode {inode_number}")))?
       .to_owned()
       .ok_or(Errno::ENOENT(format!("virtfs: read_payload: payload does not exist for inode {inode_number}")))?
       .to_owned()
     )
  }
}

// vim:ts=2 sw=2
