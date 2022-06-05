use std::fs::File;
use std::process::Command;

use chrono::{DateTime, NaiveDateTime, Utc};
use clap::Parser;
use fancy_regex::Regex;
use itertools::Itertools;
use std::io::{Read, Write};

use crate::eunix::devfs::DeviceFilesystem;
use crate::eunix::fs::{FilesystemType, VINode};
use crate::eunix::kernel::{Times, ROOT_GID, ROOT_UID};
use crate::util::{self, unixtime};
use crate::{
  eunix::{
    e5fs::E5FSFilesystem,
    fs::{AddressSize, FileModeType, FileStat, Filesystem, VFS},
    kernel::{Args, Errno, Kernel},
  },
  machine::VirtualDeviceType,
};

pub const EXIT_ENOENT: AddressSize = 127;
pub const EXIT_SUCCESS: AddressSize = 0;
pub const EXIT_FAILURE: AddressSize = 1;

// FS reading stuff

pub fn ls(args: Args, kernel: &mut Kernel) -> AddressSize {
  if let Some(pathname) = args.get(1) {
    let _parent_dir = match VFS::parent_dir(pathname) {
        Ok(parent_dir) => parent_dir,
        Err(Errno::EINVAL(message)) => {
          println!("ls: invalid path: {message}");
          return 1;
        },
        Err(errno) => {
          println!("ls: invalid path: {errno:?}");
          return 1;
        },
    };

    let dir = match kernel.vfs.read_dir(&pathname) {
      Ok(dir) => dir,
      Err(Errno::ENOTDIR(_)) => {
        println!("ls: not a directory: {pathname}");
        return 1;
      },
      Err(errno) => {
        println!("ls: unexpected error: {errno:?}");
        return 1;
      }
    };

    for (child_name, _) in dir.entries {
      let child_pathname = format!("{pathname}/{child_name}");
      let vinode = kernel
        .vfs
        .lookup_path(&child_pathname)
        .expect(&format!("ls: we know that {child_pathname} exists"));

      // Print file type
      match vinode.mode.file_type().try_into().unwrap() {
        FileModeType::Dir => print!("d"),
        FileModeType::File => print!("-"),
        FileModeType::Sys => print!("s"),
        FileModeType::Block => print!("b"),
        FileModeType::Char => print!("c"),
      }

      // Print file permissions
      // User - read
      if util::get_bit_at(vinode.mode.user(), 2) {
        print!("r");
      } else {
        print!("-");
      }
      // User - write
      if util::get_bit_at(vinode.mode.user(), 1) {
        print!("w");
      } else {
        print!("-");
      }
      // User - execute
      if util::get_bit_at(vinode.mode.user(), 0) {
        print!("x");
      } else {
        print!("-");
      }
      // group - read
      if util::get_bit_at(vinode.mode.group(), 2) {
        print!("r");
      } else {
        print!("-");
      }
      // group - write
      if util::get_bit_at(vinode.mode.group(), 1) {
        print!("w");
      } else {
        print!("-");
      }
      // group - execute
      if util::get_bit_at(vinode.mode.group(), 0) {
        print!("x");
      } else {
        print!("-");
      }
      // others - read
      if util::get_bit_at(vinode.mode.others(), 2) {
        print!("r");
      } else {
        print!("-");
      }
      // others - write
      if util::get_bit_at(vinode.mode.others(), 1) {
        print!("w");
      } else {
        print!("-");
      }
      // others - execute
      if util::get_bit_at(vinode.mode.others(), 0) {
        print!("x");
      } else {
        print!("-");
      }

      print!("\t");

      // Links count
      print!("{}", vinode.links_count);

      print!("\t");

      // User and group owners
      let user = kernel
        .uid_map
        .get(&vinode.uid)
        .unwrap_or(&format!("{}", vinode.uid))
        .clone();
      let group = kernel
        .gid_map
        .get(&vinode.gid)
        .unwrap_or(&format!("{}", vinode.gid))
        .clone();
      print!("{user} {group}");

      print!("\t");

      print!("{}", vinode.file_size);

      print!("\t");

      // Date and time
      // Create a NaiveDateTime from the timestamp
      let naive = NaiveDateTime::from_timestamp(vinode.mtime as i64, 0);

      // Create a normal DateTime from the NaiveDateTime
      let datetime: DateTime<Utc> = DateTime::from_utc(naive, Utc);

      // Format the datetime how you want
      let human_readable_date = datetime.format("%Y-%m-%d %H:%M:%S");
      print!("{}", human_readable_date);

      print!("\t");

      // Finally, file name, and newline for the next
      println!("{}", child_name);
    }
    0
  } else {
    1
  }
}

pub fn stat(args: Args, kernel: &mut Kernel) -> AddressSize {
  if let Some(pathname) = args.get(1) {
    println!("pathname_before_pass_to_bin_stat: {pathname}");
    let FileStat {
      mode,
      size,
      inode_number,
      links_count,
      uid,
      gid,
      block_size,
      atime,
      mtime,
      ctime,
      btime,
    } = match kernel.vfs.stat(&pathname) {
      Ok(stat) => stat,
      Err(Errno::ENOENT(_)) => {
        println!("stat: {pathname}: No such file or directory");
        return EXIT_ENOENT;
      },
      Err(errno) => {
        println!("stat: unexpected error: {errno:?}");
        return EXIT_FAILURE;
      }
    };
    let blocks_count = size.checked_div(block_size).unwrap_or(0);
    let file_type: FileModeType = mode.file_type().try_into().expect("should succeed");
    let file_mode_raw = mode.0;
    let atime_human =
      DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(atime as i64, 0), Utc)
      .format("%Y-%m-%d %H:%M:%S.%f");
    let mtime_human =
      DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(mtime as i64, 0), Utc)
      .format("%Y-%m-%d %H:%M:%S.%f");
    let ctime_human =
      DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(ctime as i64, 0), Utc)
      .format("%Y-%m-%d %H:%M:%S.%f");
    let btime_human =
      DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(btime as i64, 0), Utc)
      .format("%Y-%m-%d %H:%M:%S.%f");
    let user = kernel
      .uid_map
      .get(&uid)
      .unwrap_or(&String::from("<no name>"))
      .clone();
    let group = kernel
      .gid_map
      .get(&gid)
      .unwrap_or(&String::from("<no name>"))
      .clone();
    println!("  File: {pathname}");
    println!("  Size: {size}\tBlocks: {blocks_count}\t{file_type}");
    println!("Device: <unknown>\tInode: {inode_number}\tLinks: {links_count}");
    println!("Access: {file_mode_raw:o}\tUid: ({uid}/{user})\tGid: ({gid}/{group})");
    println!("Access: {atime_human}");
    println!("Modify: {mtime_human}");
    println!("Change: {ctime_human}");
    println!(" Birth: {btime_human}");
    EXIT_SUCCESS
  } else {
    EXIT_FAILURE
  }
}

pub fn df(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      EXIT_SUCCESS
    },
  }
}

pub fn du(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("du: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      EXIT_SUCCESS
    },
  }
}

pub fn cat(args: Args, kernel: &mut Kernel) -> AddressSize {
  // #[derive(Debug, Parser)]
  // struct BinArgs {
  //   pathname: String,
  // }
  
  if args[1..].is_empty() {
    println!("cat: no files to concatenate");
    return 1;
  }

  let mut concatenated_bytes = Vec::new();

  for pathname in args[1..].to_vec() {
    // For every pathname check for errors and return or append bytes to result
    let mut bytes = match kernel.vfs.read_file(&pathname, AddressSize::MAX) {
        Ok(bytes) => bytes,
        Err(Errno::ENOENT(_)) => {
          println!("cat: {pathname}: No such file or directory");
          return EXIT_ENOENT;
        },
        Err(Errno::EISDIR(_)) => {
          println!("cat: {pathname}: Is a directory");
          return EXIT_FAILURE;
        },
        Err(errno) => {
          println!("cat: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
    };
    concatenated_bytes.append(&mut bytes);
  }

  let utf8_string = match std::str::from_utf8(&concatenated_bytes) {
      Ok(utf8_string) => utf8_string,
      Err(utf8error) => {
        println!("cat: can't parse utf8: {utf8error}");
        return EXIT_FAILURE;
      },
  };

  println!("{utf8_string}");

  0
}

// FS writing stuff

pub fn mkfs_e5fs(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    #[clap(short, long, default_value_t = 4096)]
    block_data_size: AddressSize,

    #[clap(short, long, default_value_t = 0.1)]
    inode_table_percentage: f32,

    device_pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: invalid arguments: {message}");
      1
    }
    Ok(parsed_args) => {
      let dev_pathname = parsed_args.device_pathname;
      let (mount_point, internal_pathname) = kernel.vfs.match_mount_point(&dev_pathname).unwrap();
      let mounted_fs = kernel.vfs.mount_points.get_mut(&mount_point).expect("VFS::lookup_path: we know that mount_point exist"); 

      let device_realpath = if mounted_fs.r#type == FilesystemType::devfs {
        match mounted_fs
          .driver
          .as_any()
          .downcast_ref::<DeviceFilesystem>()
          .expect("we know that mounted_fs.driver === instanceof DeviceFilesystem")
          .device_by_pathname(&internal_pathname) 
        {
            Ok(realpath) => realpath,
            Err(Errno::ENOENT(_)) => {
              println!("mkfs.e5fs: {dev_pathname}: No such file or directory");
              return EXIT_ENOENT;
            },
            Err(errno) => {
              println!("mkfs.e5fs: unexpected error: {errno:?}");
              return EXIT_FAILURE;
            },
        }
      } else {
        println!("mkfs.e5fs: {dev_pathname}: Not a device");
        return EXIT_FAILURE;
      };

      match E5FSFilesystem::mkfs(
        &device_realpath, 
        parsed_args.inode_table_percentage, 
        parsed_args.block_data_size
      ) {
        Ok(_) => EXIT_SUCCESS,
        Err(errno) => {
          println!("mkfs.e5fs: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      }
    }
  }
}

pub fn mkdir(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkdir: invalid arguments: {message}");
      1
    },
    Ok(BinArgs { pathname }) => {
      match kernel.vfs.create_dir(&pathname) {
        Ok(_) => EXIT_SUCCESS,
        Err(Errno::ENOENT(_)) => {
          println!("mkdir: cannot create directory: '{pathname}': No such file or directory");
          EXIT_ENOENT
        },
        Err(errno) => {
          println!("mkdir: unexpected error: {errno:?}");
          EXIT_FAILURE
        },
      }
    },
  }
}

pub fn rmdir(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("rmdir: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      EXIT_SUCCESS
    },
  }
}

pub fn touch(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("touch: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      match kernel.vfs.lookup_path(&pathname) {
        Ok(vinode) => {
        match kernel.vfs.change_times(&pathname, Times {
          atime: unixtime(),
          mtime: vinode.mtime,
          ctime: unixtime(),
          btime: vinode.btime,
        }) {
          Ok(_) => EXIT_SUCCESS,
          Err(errno) => {
            println!("touch: unexpected error: {errno:?}");
            EXIT_FAILURE
          },
        }
        },
        Err(Errno::ENOENT(_)) => {
          match VFS
            ::parent_dir(&pathname)
            .and_then(|parent_pathname| kernel.vfs.lookup_path(&parent_pathname))
          {
            Ok(_) => {
              match kernel.vfs.create_file(&pathname) {
                Ok(_) => EXIT_SUCCESS,
                Err(errno) => {
                  println!("touch: unexpected error: {errno:?}");
                  EXIT_FAILURE
                },
              }
            },
            Err(Errno::ENOENT(_)) => {
              println!("touch: cannot touch '{pathname}': No such file or directory");
              EXIT_ENOENT
            },
            Err(errno) => {
              println!("touch: unexpected error: {errno:?}");
              EXIT_FAILURE
            },
          }
        },
        Err(errno) => {
          println!("touch: unexpected error: {errno:?}");
          EXIT_FAILURE
        },
      }
    },
  }
}

pub fn rm(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    #[clap(short, long, takes_value = false)]
    recurse: bool,

    // #[clap(short, long)]
    // force: bool,

    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("rm: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname, recurse }) => {
      let vinode = match kernel.vfs.lookup_path(&pathname) {
        Ok(vinode) => vinode,
        Err(Errno::ENOENT(_)) => {
          println!("rm: cannot remove '{pathname}': No such file or directory");
          return EXIT_ENOENT;
        },
        Err(errno) => {
          println!("rm: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      };

      // Just a file case
      if vinode.mode.file_type() != FileModeType::Dir as u8 {
        return match kernel.vfs.remove_file(&pathname) {
          Ok(()) => EXIT_SUCCESS,
          Err(errno) => {
            println!("rm: unexpected error: {errno:?}");
            EXIT_FAILURE
          },
        }
      }

      // Directory case
      if !recurse {
        println!("rm: cannot remove '{pathname}': Is a directory");
        return EXIT_FAILURE;
      }

      // Recurse
      match kernel.vfs.read_dir(&pathname) {
        Ok(dir) => {
          for (name, _) in dir
            .entries
            .into_iter()
            .filter(|(name, _)| name != "." && name != "..")
          {
            let cloned_arg0 = args.get(0).unwrap().clone();
            let new_pathname = format!("{pathname}/{name}");
            println!("rm: descending into '({new_pathname})'");
            let exit_status = rm(vec![cloned_arg0, String::from("-r"), new_pathname], kernel);
            if exit_status != EXIT_SUCCESS {
              return exit_status;
            }
          }
          return match kernel.vfs.remove_file(&pathname) {
            Ok(()) => EXIT_SUCCESS,
            Err(errno) => {
              println!("rm: unexpected error: {errno:?}");
              EXIT_FAILURE
            },
          } 
        },
        Err(errno) => {
          println!("rm: unexpected error: {errno:?}");
          EXIT_FAILURE
        },
      }
    },
  }
}

pub fn mv(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    source_pathname: String,
    target_pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mv: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { source_pathname, target_pathname }) => {
      EXIT_SUCCESS
    },
  }
}

pub fn cp(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    source_pathname: String,
    target_pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("cp: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { source_pathname, target_pathname }) => {
      let source_vinode = match kernel.vfs.lookup_path(&source_pathname) {
        Ok(vinode) => vinode,
        Err(Errno::ENOENT(_)) => {
          println!("cp: {source_pathname}: No such file or directory");
          return EXIT_ENOENT;
        },
        Err(errno) => {
          println!("cp: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      };

      // Guard for target already existing
      if let Ok(_) = kernel.vfs.lookup_path(&target_pathname) {
        println!("cp: {target_pathname}: Already exists");
        return EXIT_FAILURE;
      }

      println!("cp: source_pathname is {source_pathname} source_vinode.mode is {:03b}", source_vinode.mode.file_type());

      // Main part - base file case or recurse
      if source_vinode.mode.file_type() == FileModeType::File as u8 {
        println!("cp: file case");
        let source_bytes = kernel.vfs.read_file(&source_pathname, AddressSize::MAX).unwrap();
        kernel.vfs.create_file(&target_pathname).unwrap();
        kernel.vfs.write_file(&target_pathname, &source_bytes).unwrap();
        EXIT_SUCCESS
      } else {
        println!("cp: dir case (creating dir: {target_pathname})");
        kernel.vfs.create_dir(&target_pathname).unwrap();
        let dir = kernel.vfs.read_dir(&source_pathname).unwrap();
        for (name, _) in dir
          .entries
          .iter()
          .filter(|(name, _)| **name != "." && **name != "..") 
        {
          let cloned_arg0 = args.get(0).unwrap().clone();
          let new_source_pathname = format!("{source_pathname}/{name}");
          let new_target_pathname = format!("{target_pathname}/{name}");
          println!("cp: descending into '({source_pathname})'");
          let exit_status = cp(vec![cloned_arg0, new_source_pathname, new_target_pathname], kernel);
          if exit_status != EXIT_SUCCESS {
            return exit_status;
          }
        }
        EXIT_SUCCESS 
      }
    },
  }
}

pub fn write(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
    text: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: invalid arguments: {message}");
      1
    },
    Ok(BinArgs { pathname, text }) => {
      let bytes = text.as_bytes();
      match kernel.vfs.write_file(&pathname, bytes) {
        Ok(_) => EXIT_SUCCESS,
        Err(Errno::ENOENT(_)) => {
          println!("write: {pathname}: No such file or directory");
          return EXIT_ENOENT;
        },
        Err(Errno::EISDIR(_)) => {
          println!("write: {pathname}: Is a directory");
          return EXIT_FAILURE;
        },
        Err(errno) => {
          println!("write: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      }
    },
  }
}

pub fn ed(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("ed: parse error: {message}");
      1
    },
    Ok(BinArgs { pathname }) => {
      // Read file
      let bytes = match kernel.vfs.read_file(&pathname, AddressSize::MAX) {
        Ok(bytes) => bytes,
        Err(Errno::ENOENT(_)) => {
          println!("ed: {pathname}: No such file or directory");
          return EXIT_ENOENT;
        },
        Err(Errno::EISDIR(_)) => {
          println!("ed: {pathname}: Is a directory");
          return EXIT_FAILURE;
        },
        Err(errno) => {
          println!("ed: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      };
      
      // Edit file
      let editor = std::env::var("EDITOR").expect("EDITOR env var must be set");
      let mut file_path = std::env::temp_dir();
      file_path.push("eunix_editor_file");

      match File::create(&file_path)
        .and_then(|mut file| file.write_all(&bytes)) 
      {
        Ok(_) => {
        },
        Err(message) => {
          println!("ed: error while creating platform-provided temp file: {message:#?}");
          return EXIT_FAILURE;
        },
      }
      

      if let Err(message) = Command::new(editor)
          .arg(&file_path)
          .status() 
      {
        println!("ed: error while opening platform-provided editor: {message:#?}");
        return EXIT_FAILURE;
      }

      let mut edited_bytes = Vec::new();

      match File::open(&file_path)
        .and_then(|mut file| file.read_to_end(&mut edited_bytes)) 
      {
        Ok(_) => {
        },
        Err(message) => {
          println!("ed: error while reading back edited platform-provided temp file: {message:#?}");
          return EXIT_FAILURE;
        },
      }

      // Write file back
      return match kernel.vfs.write_file(&pathname, &edited_bytes) {
        Ok(_) => EXIT_SUCCESS,
        Err(errno) => {
          println!("ed: unexpected error: {errno:?}");
          EXIT_FAILURE
        },
      }
    },
  }
}

pub fn chmod(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    mode: String,
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("chmod: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname, mode: new_mode_string }) => {
      let old_mode = match kernel.vfs.lookup_path(&pathname) {
        Ok(vinode) => vinode.mode,
        Err(Errno::ENOENT(_)) => {
          println!("chmod: {pathname}: No such file or directory");
          return EXIT_ENOENT;
        },
        Err(errno) => {
          println!("chmod: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      };

      if !Regex::new("^[0-7]{3}$")
        .unwrap()
        .is_match(&new_mode_string)
        .unwrap()
      {
        println!("chmod: invalid mode: '{new_mode_string}'");
        return EXIT_FAILURE;
      }

      let user: AddressSize = new_mode_string.chars().map(|c| c.to_digit(8)).nth(0).unwrap().unwrap();
      let group: AddressSize = new_mode_string.chars().map(|c| c.to_digit(8)).nth(1).unwrap().unwrap();
      let others: AddressSize = new_mode_string.chars().map(|c| c.to_digit(8)).nth(2).unwrap().unwrap();

      let new_mode = old_mode
        .with_user(user as u8)
        .with_group(group as u8)
        .with_others(others as u8);

      match kernel.vfs.change_mode(&pathname, new_mode) {
        Ok(_) => EXIT_SUCCESS,
        Err(errno) => {
          println!("chmod: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      }
    },
  }
}

pub fn chown(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    new_owners_string: String,
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("chown: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname, new_owners_string }) => {
      let VINode {
        uid,
        gid,
        ..
      } = match kernel.vfs.lookup_path(&pathname) {
        Ok(vinode) => vinode,
        Err(Errno::ENOENT(_)) => {
          println!("chown: {pathname}: No such file or directory");
          return EXIT_ENOENT;
        },
        Err(errno) => {
          println!("chown: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      };

      // Parse names - default to current
      let user_name = new_owners_string
        .split(":")
        .nth(0)
        .unwrap_or(kernel.uid_map.get(&kernel.current_uid).unwrap());
      let group_name = new_owners_string
        .split(":")
        .nth(1)
        .unwrap_or(kernel.uid_map.get(&kernel.current_gid).unwrap());

      // Guard user
      let uid = if let Some(uid) = kernel
        .uid_map
        .iter()
        .find(|(_, name)| user_name == *name)
        .map(|(id, _)| *id)
      {
        uid
      } else {
        println!("chown: invalid user: '{new_owners_string}'");
        return EXIT_FAILURE;
      };

      // Guard group
      let gid = if let Some(gid) = kernel
        .gid_map
        .iter()
        .find(|(_, name)| group_name == *name)
        .map(|(id, _)| *id)
      {
        gid
      } else {
        println!("chown: invalid group: '{new_owners_string}'");
        return EXIT_FAILURE;
      };

      match kernel.vfs.change_owners(&pathname, uid, gid) {
        Ok(_) => EXIT_SUCCESS,
        Err(errno) => {
          println!("chown: unexpected error: {errno:?}");
          EXIT_FAILURE
        },
      }
    },
  }
}

// System related stuff
pub fn uname(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { }) => {
      println!("Eunix");
      EXIT_SUCCESS
    },
  }
}

pub fn lsblk(args: Args, kernel: &mut Kernel) -> AddressSize {
  let device_table = kernel.devices();
  let mount_points = &kernel.vfs.mount_points;
  println!("{device_table:#?}");
  println!("mount_points: {mount_points:#?}");
  EXIT_SUCCESS
}

pub fn dumpe5fs(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      // let (mount_point, internal_path) = kernel.vfs.match_mount_point(&pathname).unwrap();
      // let mounted_fs = kernel.vfs.mount_points.get_mut(&mount_point).expect("VFS::lookup_path: we know that mount_point exist");  
      //
      // mounted_fs.driver.as_any().downcast_mut()
      //
      // println!("{device_table:#?}");
      // println!("mount_points: {mount_points:#?}");
      EXIT_SUCCESS
    }
  }
}

pub fn mount(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    #[clap(short = 't', long, default_value_t = FilesystemType::e5fs)]
    filesystem_type: FilesystemType,

    source: String,
    target: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mount: error: {message}");
      1
    }
    Ok(BinArgs {
      filesystem_type,
      source,
      target,
    }) => match kernel.mount(&source, &target, filesystem_type) {
      Ok(_) => 0,
      Err(Errno::EINVAL(message)) => {
        println!("mount: error: {message}");
        1
      }
      Err(_) => unreachable!(),
    },
  }
}

// User related stuff

pub fn id(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("id: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { }) => {
      let Kernel { current_uid, current_gid, .. } = kernel;
      let current_username = kernel
        .uid_map
        .get(current_uid)
        .unwrap_or(&String::from("<no name>"))
        .clone();
      let current_groupname = kernel
        .gid_map
        .get(current_gid)
        .unwrap_or(&String::from("<no name>"))
        .clone();
      let current_sgids_string = kernel
        .current_sgids
        .iter()
        .map(|sgid| {
          let groupname = kernel
            .gid_map
            .get(sgid)
            .unwrap_or(&String::from("<no name>"))
            .clone();
          format!("{sgid}({groupname})")
        })
        .join(",");

      println!("uid={current_uid}({current_username}) gid={current_gid}({current_groupname}) groups={current_sgids_string}");
      EXIT_SUCCESS
    },
  }
}

pub fn whoami(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("whoami: invalid arguments: {message}");
      1
    }
    Ok(BinArgs {}) => {
      let user_name = kernel
        .gid_map
        .get(&kernel.current_uid)
        .unwrap_or(&format!("<no name>({})", kernel.current_uid))
        .clone();

      println!("{user_name}");

      EXIT_SUCCESS
    },
  }
}

pub fn su(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    user: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("su: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { user }) => {
      if let Some(uid) = kernel
        .uid_map
        .iter()
        .find(|(_, name)| user == **name)
        .map(|(id, _)| *id)
      {
        kernel.current_uid = uid;
        EXIT_SUCCESS
      } else {
        println!("su: user '{user}' does not exist; you might want to reread /etc/passwd");
        EXIT_FAILURE
      }
    },
  }
}

pub fn useradd(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      EXIT_SUCCESS
    },
  }
}

pub fn usermod(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      EXIT_SUCCESS
    },
  }
}

pub fn userdel(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      EXIT_SUCCESS
    },
  }
}

pub fn groupmod(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      EXIT_SUCCESS
    },
  }
}

pub fn groupdel(args: Args, kernel: &mut Kernel) -> AddressSize {
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("mkfs.e5fs: invalid arguments: {message}");
      1
    },
    Ok(BinArgs { pathname }) => {
      EXIT_SUCCESS    
    },
  }
}

// vim:ts=2 sw=2
