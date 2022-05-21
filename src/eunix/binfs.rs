// use std::any::Any;
// use std::collections::BTreeMap;
// use std::time::SystemTime;
//
// use petgraph::Graph;
// use petgraph::graph::NodeIndex;
// use uuid::Uuid;
//
// use crate::eunix::kernel::Kernel;
// use crate::machine::VirtualDeviceType;
// use crate::eunix::fs::Filesystem;
// use crate::util::unixtime;
//
// use super::fs::{AddressSize, VDirectoryEntry, VINode, VDirectory, VFS, FileMode, FileStat, FileModeType};
// use super::kernel::{Errno, KernelDeviceTable};
//
// pub struct DirectoryEntry<'a> { inode_address: AddressSize,
//   name: &'a str,
//   next_dir_entry_offset: AddressSize,
// }
//
// #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
// pub struct INode {
//   mode: FileMode,
//   links_count: AddressSize,
//   uid: u16,
//   gid: u16,
//   file_size: AddressSize,
//   atime: u32,
//   mtime: u32,
//   ctime: u32,
//   number: AddressSize,
// }
// impl From<INode> for VINode {
//   fn from(inode: INode) -> Self {
//     Self {
//       mode: inode.mode,
//       links_count: inode.links_count,
//       file_size: inode.file_size,
//       uid: inode.uid,
//       gid: inode.gid,
//       atime: inode.atime,
//       ctime: inode.ctime,
//       mtime: inode.mtime,
//       number: inode.number,
//     }
//   }
// }
//
// pub struct Superblock {
//   filesystem_type: [u8; 255],
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
//
// pub struct Block<'a> {
//   is_free: bool,
//   data: &'a [u8],
//   next_block: AddressSize,
// }
//
// pub type BinFsBinary = dyn Fn(Vec<String>) -> AddressSize;
//
// pub struct BinFsEntry {
//   name: String,
//   contents: BinFsContents,
// }
// impl BinFsEntry {
//   fn new(name: &str, contents: BinFsContents) -> Self {
//     BinFsEntry {
//       name: name.to_owned(),
//       contents,
//     }
//   }
// }
// pub enum BinFsContents {
//   Binary(Box<BinFsBinary>),
//   Directory,
// }
// // impl PartialEq for BinFsEntry {
// //     fn eq(&self, other: &Self) -> bool {
// //       self.name == other.
// //       match (&self, &other) {
// //         (BinFsContents::Binary(_, _), BinFsContents::Directory(_)) => false,
// //         (BinFsContents::Directory(_), BinFsContents::Binary(_, _)) => false,
// //         (BinFsContents::Binary(a, _), BinFsContents::Binary(b, _)) => a == b,
// //         (BinFsContents::Directory(a), BinFsContents::Directory(b)) => a == b,
// //       }
// //     }
// // }
// // impl Eq for BinFsEntry {}
//
// pub type BinFsTree = Graph<BinFsEntry, ()>;
// pub struct BinFilesystem {
//   tree: BinFsTree,
//   inodes: Vec<INode>,
//   root_index: NodeIndex,
// }
//
// impl BinFilesystem {
//   pub fn new() -> Self {
//     let tree = Graph::new();
//     let root_index = tree.add_node(
//       BinFsEntry::new("/", BinFsContents::Directory)
//     );
//
//     let binfs = Self {
//       inodes: Vec::new(),
//       tree,
//       root_index,
//     };
//
//     binfs
//   }
//
//   pub fn create_directory(&mut self, pathname: &str) -> Result<(), Errno> {
//     let pathname = VFS::split_path(pathname)?;
//     let (everything_else, final_component) = pathname.clone();
//
//     let parent = match everything_else {
//       xs if xs.is_empty() => self.root_index,
//       xs => {
//         let find_target_dir = |tree: BinFsTree, current_name: &str, current_index: NodeIndex| -> NodeIndex {
//           // Find next node by name among neighbors
//           if let Some(index) = tree
//             .neighbors(current_index)
//             .find(|&index| tree[index].name == current_name)
//           {
//             match tree[index].contents {
//               BinFsContents::Binary => (),
//             };
//           } else {
//
//           }
//           todo!()
//         };
//         // let root = self.tree.neighbors()
//
//         todo!()
//       },
//     };
//
//     // match self.tree[self.root_index] {
//     // BinFsEntry::Binary(_, _) => 
//     //   return Err(
//     //     Errno::EBADFS(
//     //       "binfs: create_directory: root is a file (you should not see that)"
//     //     )
//     //   )
//     // },
//     //
//     todo!()
//   }
// }
//
// impl Filesystem for BinFilesystem {
//   fn as_any(&mut self) -> &mut dyn Any {
//     self
//   }
//
//   fn read_file(&mut self, _pathname: &str, _count: AddressSize) -> Result<Vec<u8>, Errno> {
//     Err(Errno::EPERM("binfs: read_file: permission denied"))
//   }
//
//   fn write_file(&mut self, _pathname: &str, _data: &[u8]) -> Result<VINode, Errno> {
//     Err(Errno::EPERM("binfs: write_file: permission denied"))
//   }
//
//   fn read_dir(&mut self, pathname: &str) -> Result<VDirectory, Errno> {
//     Err(Errno::EPERM("binfs: read_dir: permission denied"))
//   }
//
//   // Поиск файла в файловой системе. Возвращает INode фала.
//   // Для VFS сначала матчинг на маунт-поинты и вызов lookup_path("/mount/point") у конкретной файловой системы;
//   // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
//   fn lookup_path(&mut self, pathname: &str) -> Result<VINode, Errno> {
//     let (_everything_else, final_component) = VFS::split_path(pathname)?;
//     let dir = self.read_dir("/")?; // TODO: FIXME: magic string
//
//     let inode_number = if final_component == "." {
//       0
//     } else {
//       dir.entries.get(&final_component).ok_or(Errno::ENOENT("no such file or directory 2"))?.inode_number
//     };
//     
//     self.inodes
//       .iter()
//       .find(|inode| inode.number == inode_number)
//       .map(|&inode| inode.into())
//       .ok_or(Errno::EIO("binfs::lookup_path: can't find inode from dir"))
//   }
//
//   fn name(&self) -> &'static str {
//     "binfs"
//   }
//
//   fn create_file(&mut self, pathname: &str)
//     -> Result<VINode, Errno> {
//     Err(Errno::EPERM("operation not permitted"))
//   }
//
//   fn stat(&mut self, pathname: &str)
//     -> Result<super::fs::FileStat, Errno> {
//     let VINode {
//       mode,
//       file_size,
//       links_count,
//       uid,
//       gid,
//       number,
//       ..
//     } = self.lookup_path(pathname)?; 
//
//     Ok(FileStat {
//       mode,
//       size: file_size,
//       inode_number: number,
//       links_count,
//       uid,
//       gid,
//       block_size: 0, // TODO: FIXME: magic number
//     })
//   }
//
//   fn change_mode(&mut self, pathname: &str, mode: super::fs::FileMode)
//     -> Result<(), Errno> {
//     Err(Errno::EPERM("operation not permitted"))
//   }
// }
//
// // impl DeviceFilesystem {
// //   fn mkfs(percent_inodes: u32, block_size: AddressSize) {}
// // }
//
// // vim:ts=2 sw=2
