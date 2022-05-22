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
use crate::eunix::fs::NOBODY;
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
    self.entries.remove(name).ok_or(Errno::ENOENT("no such file in directory"))?;
    Ok(())
  }
}

#[derive(Debug, Clone)]
enum VirtFsINodePayload<T: VirtFsFile> {
  Directory(Directory),
  File(Box<T>),
}

impl<T: VirtFsFile> fmt::Display for VirtFsINodePayload<T> {
  fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
    match self {
      VirtFsINodePayload::Directory(dir) => write!(formatter, "{:?}", dir),
      VirtFsINodePayload::File(file) => write!(formatter, "{}", file),
    }
  }
}

impl<T: VirtFsFile> Default for VirtFsINodePayload<T> {
  fn default() -> Self {
    VirtFsINodePayload::File(Box::new(T::default()))
  }
}

#[derive(Debug, Clone)]
pub struct INode<T: VirtFsFile> {
  mode: FileMode,
  links_count: AddressSize,
  uid: Id,
  gid: Id,
  file_size: AddressSize,
  atime: u32,
  mtime: u32,
  ctime: u32,
  payload: VirtFsINodePayload<T>,
  number: AddressSize,
}

impl<T: VirtFsFile> fmt::Display for INode<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self)
  }
}

impl<T: VirtFsFile> Default for INode<T> {
  fn default() -> Self {
    Self {
      mode: FileMode::default(),
      links_count: 0,
      file_size: 0,
      uid: NOBODY,
      gid: NOBODY,
      atime: 0,
      mtime: 0,
      ctime: 0,
      payload: VirtFsINodePayload::default(),
      number: 0,
    }
  }
}

impl<T: VirtFsFile> From<INode<T>> for VINode {
  fn from(inode: INode<T>) -> Self {
    Self {
      mode: inode.mode,
      links_count: inode.links_count,
      file_size: inode.file_size,
      uid: inode.uid,
      gid: inode.gid,
      atime: inode.atime,
      ctime: inode.ctime,
      mtime: inode.mtime,
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
pub struct Superblock<T: VirtFsFile> {
  pub filesystem_size: AddressSize, // in blocks
  pub blocks_count: AddressSize,
  pub inodes: Vec<INode<T>>,
}


impl<T: VirtFsFile> Superblock<T> {
  fn new() -> Self {
    Self {
      filesystem_size: 0,
      blocks_count: 0,
      inodes: Vec::new(),
    }
  }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Block {
  data: Vec<u8>,
}

pub struct VirtFsFilesystem<T: VirtFsFile> {
  pub superblock: Superblock<T>,
  name: String,
}

impl<T: VirtFsFile> Filesystem for VirtFsFilesystem<T> {
  fn as_any(&mut self) -> &mut dyn Any {
    unimplemented!("virtfs: as_any for virtfs is undefined")
  }

  fn create_file(&mut self, pathname: &str)
    -> Result<VINode, Errno> {
    // Regex matching final_component of path (+ leading slash)
    let (everything_else, dirent_name) = VFS::split_path(pathname)?;
    // NOTICE: there may be problems/conflicts with leading / insertion
    //         in intermadiate path representations
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
       return Err(Errno::EINVAL("file already exists"));
    }

    // Allocate inode
    let inode = INode::<T>::default();

    // Push allocated to dir
    dir.insert(inode.number, dirent_name.as_str())?;

    // Write dir
    self.write_dir(&dir, dir_inode.number)?;

    Ok(inode.into())
  } 

  fn read_file(&mut self, pathname: &str, _count: AddressSize)
    -> Result<Vec<u8>, Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    let file = self
      .read_from_file(inode_number)?;

    Ok(
      format!("{}", file)
        .as_bytes()
        .to_owned()
    )
  }

  fn write_file(&mut self, pathname: &str, data: &[u8])
    -> Result<VINode, Errno> {
      todo!("Accept callbacks for read and write from the instantiator")
  } 

  fn read_dir(&mut self, pathname: &str)
    -> Result<VDirectory, Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    let dir = self.read_dir_from_inode(inode_number)?;

    Ok(dir.into())
  }

  fn stat(&mut self, pathname: &str) 
    -> Result<FileStat, Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    let INode {
      mode,
      file_size,
      links_count,
      uid,
      gid,
      ..
    } = self.read_inode(inode_number)?;

    Ok(FileStat {
      mode,
      size: file_size,
      inode_number,
      links_count,
      uid,
      gid,
      block_size: 0
    })
  }

  fn change_mode(&mut self, pathname: &str, mode: FileMode)
    -> Result<(), Errno> {
    let inode_number = self.lookup_path(pathname)?.number;
    self.write_mode(inode_number, mode)
  }

  // Поиск файла в файловой системе. Возвращает INode фала.
  // Для VFS сначала матчинг на маунт-поинты и вызов lookup_path("/mount/point") у конкретной файловой системы;
  // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
  fn lookup_path(&mut self, pathname: &str)
    -> Result<VINode, Errno> {
    let pathname = VFS::split_path(pathname)?;
    let (everything_else, final_component) = pathname.clone();
    let mut inode: INode<T> = self.read_inode(ROOT_INODE_NUMBER)?;

    // Base case
    if pathname == (Vec::new(), String::from("/")) {
      let inode = self.read_inode(ROOT_INODE_NUMBER)?;
      return Ok(inode.into());
    };

    fn is_dir(inode: VINode) -> bool {
      // TODO: critical bug: inode mode and... nevermind, the present is correct
      let filetype = inode.mode.r#type();
      filetype == FileModeType::Dir as u8
    }

    // TODO: add 'blocks' vector to the VirtFsFilesystem: Vec<T>, indexed by inodes with payload_index
    fn find_dir<T: VirtFsFile>(virtfs: &mut VirtFsFilesystem<T>, everything_else: Vec<String>, initial_inode: &INode<T>) -> Result<INode<T>, Errno> {
      let mut inode = initial_inode.clone();

      let mut everything_else = VecDeque::from(everything_else);
      // TODO: pass inode to read_dir_from_inode
      while everything_else.len() > 0 {
        if !is_dir(inode.clone().into()) {
          return Err(Errno::ENOTDIR("virtfs.lookup_path: not a directory (find_dir)"))
        }

        let piece = everything_else.pop_front().unwrap();
        let dir = virtfs.read_dir_from_inode(inode.number)?;
        if let Some(entry) = dir.entries.get(&piece.to_owned()) {
          inode = virtfs.read_inode(entry.inode_number)?;
        } else {
          return Err(Errno::ENOENT("virtfs.lookup_path: no such file or directory"))
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
        .ok_or_else(|| Errno::ENOENT("virtfs.lookup_path: no such file or directory (get(final_component))"))
        // Read its inode_number
        .and_then(|entry| self.read_inode(entry.inode_number))?
        .into()
    )
  } 

  fn name(&self) -> String { 
    self.name().clone()
  }

  fn create_dir(&mut self, pathname: &str)
    -> Result<VINode, Errno> {
    todo!()
  } 
}

impl<T: VirtFsFile> VirtFsFilesystem<T> {
  /// Construct new virtfs
  pub fn new(name: &str) -> Self {
    Self {
      superblock: Superblock::new(),
      name: name.to_owned(),
    }
  }

  pub fn name(&self) -> String {
    self.name.clone()
  }

  fn write_dir(&mut self, dir: &Directory, inode_number: AddressSize) -> Result<(), Errno> {
    if let Some(inode) = self.superblock.inodes.get_mut(inode_number as usize) {
      inode.payload = VirtFsINodePayload::Directory(dir.clone());
      Ok(())
    } else {
      Err(Errno::ENOENT("virtfs: no such file or directory"))
    }
  }

  fn read_dir_from_inode(&mut self, inode_number: AddressSize) -> Result<Directory, Errno> {
    match self.read_from_file(inode_number)? {
      VirtFsINodePayload::Directory(directory) => Ok(directory),
      VirtFsINodePayload::File(_) => Err(Errno::ENOTDIR("tried to read file from inode (TODO: inode number here), got directory")),
    }
  }

  fn read_from_file(&mut self, inode_number: AddressSize) -> Result<VirtFsINodePayload<T>, Errno> {
    let inode = self.read_inode(inode_number);

    self
       .superblock
       .inodes
       .get(inode_number as usize)
       .map(|inode| inode.payload.clone())
       .ok_or(Errno::ENOENT("virtfs: read_from_file: no such file or directory"))
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
    Ok(())
  }

  #[allow(dead_code)]
  fn read_inode(&mut self, inode_number: AddressSize) -> Result<INode<T>, Errno> {
    Ok(
      self
       .superblock
       .inodes
       .get(inode_number as usize)
       .ok_or(Errno::ENOENT("virtfs: read_inode: no such file or directory"))?
       .clone()
     )
  }
}

// vim:ts=2 sw=2
