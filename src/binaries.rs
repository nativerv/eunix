use std::fs::File;
use std::process::Command;
use crate::eunix::users::{Passwd, ParseError};

use chrono::{DateTime, NaiveDateTime, Utc};
use clap::Parser;
use fancy_regex::Regex;
use itertools::Itertools;
use sha2::{Sha256, Digest};
use std::io::{Read, Write, stdout, stdin};

use crate::eunix::devfs::DeviceFilesystem;
use crate::eunix::fs::{FilesystemType, VINode, Id, NOBODY_UID, NOBODY_GID};
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

pub const PASSWD_PATH: &'static str = "/etc/passwd";

// FS reading stuff

pub fn ls(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  if let Some(pathname) = args.get(1) {
    let _parent_dir = match VFS::parent_dir(pathname) {
        Ok(parent_dir) => parent_dir,
        Err(Errno::EINVAL(message)) => {
          println!("{arg0}: invalid path: {message}");
          return 1;
        },
        Err(errno) => {
          println!("{arg0}: invalid path: {errno:?}");
          return 1;
        },
    };

    let dir = match kernel.vfs.read_dir(&pathname) {
      Ok(dir) => dir,
      Err(Errno::ENOTDIR(_)) => {
        println!("{arg0}: not a directory: {pathname}");
        return 1;
      },
      Err(Errno::EACCES(_)) => {
        println!("{arg0}: '{pathname}': Permission denied");
        return EXIT_FAILURE
      },
      Err(errno) => {
        println!("{arg0}: unexpected error: {errno:?}");
        return 1;
      }
    };

    for (child_name, _) in dir.entries {
      let child_pathname = format!("{pathname}/{child_name}");
      let vinode = kernel
        .vfs
        .lookup_path(&child_pathname)
        .expect(&format!("{arg0}: we know that {child_pathname} exists"));

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
        .unwrap_or(&format!("<gid{}>", vinode.gid))
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
  let arg0 = args.get(0).unwrap().clone();
  if let Some(pathname) = args.get(1) {
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
        println!("{arg0}: {pathname}: No such file or directory");
        return EXIT_ENOENT;
      },
      Err(Errno::EACCES(_)) => {
        println!("{arg0}: '{pathname}': Permission denied");
        return EXIT_FAILURE
      },
      Err(errno) => {
        println!("{arg0}: unexpected error: {errno:?}");
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
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      EXIT_SUCCESS
    },
  }
}

pub fn du(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      EXIT_SUCCESS
    },
  }
}

pub fn cat(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  // #[derive(Debug, Parser)]
  // struct BinArgs {
  //   pathname: String,
  // }
  
  if args[1..].is_empty() {
    println!("{arg0}: no files to concatenate");
    return 1;
  }

  let mut concatenated_bytes = Vec::new();

  for pathname in args[1..].to_vec() {
    // For every pathname check for errors and return or append bytes to result
    let mut bytes = match kernel.vfs.read_file(&pathname, AddressSize::MAX) {
        Ok(bytes) => bytes,
        Err(Errno::ENOENT(_)) => {
          println!("{arg0}: {pathname}: No such file or directory");
          return EXIT_ENOENT;
        },
        Err(Errno::EISDIR(_)) => {
          println!("{arg0}: {pathname}: Is a directory");
          return EXIT_FAILURE;
        },
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: '{pathname}': Permission denied");
          return EXIT_FAILURE
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
    };
    concatenated_bytes.append(&mut bytes);
  }

  // Try to parse utf8 from file (most probably succeeds)
  let utf8_string = match std::str::from_utf8(&concatenated_bytes) {
      Ok(utf8_string) => utf8_string,
      Err(utf8error) => {
        println!("{arg0}: can't parse utf8: {utf8error}");
        return EXIT_FAILURE;
      },
  };

  // Guard for having '\n' at the end (for some reason gets inserted by nvim or whatnot)
  if let Some(char) = utf8_string.chars().last() && char == '\n' {
    print!("{utf8_string}")
  } else {
    println!("{utf8_string}")
  };

  0
}

// FS writing stuff

pub fn mkfs_e5fs(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
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
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(parsed_args) => {
      let dev_pathname = parsed_args.device_pathname;
      let (mount_point, internal_pathname) = kernel.vfs.match_mount_point(&dev_pathname).unwrap();
      let mounted_fs = kernel.vfs.mount_points.get_mut(&mount_point).expect("{arg0}::lookup_path: we know that mount_point exist"); 

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
              println!("{arg0}: {dev_pathname}: No such file or directory");
              return EXIT_ENOENT;
            },
            Err(errno) => {
              println!("{arg0}: unexpected error: {errno:?}");
              return EXIT_FAILURE;
            },
        }
      } else {
        println!("{arg0}: {dev_pathname}: Not a device");
        return EXIT_FAILURE;
      };

      match E5FSFilesystem::mkfs(
        &device_realpath, 
        parsed_args.inode_table_percentage, 
        parsed_args.block_data_size
      ) {
        Ok(_) => EXIT_SUCCESS,
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      }
    }
  }
}

pub fn mkdir(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    },
    Ok(BinArgs { pathname }) => {
      match kernel.vfs.create_dir(&pathname) {
        Ok(_) => EXIT_SUCCESS,
        Err(Errno::ENOENT(_)) => {
          println!("{arg0}: cannot create directory: '{pathname}': No such file or directory");
          EXIT_ENOENT
        },
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: '{pathname}': Permission denied");
          return EXIT_FAILURE
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          EXIT_FAILURE
        },
      }
    },
  }
}

pub fn rmdir(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      EXIT_SUCCESS
    },
  }
}

pub fn touch(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
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
            Err(Errno::EACCES(_)) => {
              println!("{arg0}: '{pathname}': Permission denied");
              return EXIT_FAILURE
            },
            Err(Errno::EPERM(_)) => {
              println!("{arg0}: '{pathname}': Operation not permitted");
              return EXIT_FAILURE
            },
            Err(errno) => {
              println!("{arg0}: unexpected error 1: {errno:?}");
              EXIT_FAILURE
            },
          }
        },
        Err(Errno::ENOENT(_)) => {
          println!("file {pathname} don't exist. creating it..");
          match VFS::parent_dir(&pathname)
            .and_then(|parent_pathname| kernel.vfs.lookup_path(&parent_pathname))
          {
            Ok(_) => {
              match kernel.vfs.create_file(&pathname) {
                Ok(_) => EXIT_SUCCESS,
                Err(Errno::EACCES(_)) => {
                  println!("{arg0}: '{pathname}': Permission denied");
                  return EXIT_FAILURE
                },
                Err(Errno::EPERM(_)) => {
                  println!("{arg0}: '{pathname}': Operation not permitted");
                  return EXIT_FAILURE
                },
                Err(errno) => {
                  println!("{arg0}: unexpected error 2: {errno:?}");
                  EXIT_FAILURE
                },
              }
            },
            Err(Errno::ENOENT(_)) => {
              println!("{arg0}: cannot touch '{pathname}': No such file or directory");
              EXIT_ENOENT
            },
            Err(errno) => {
              println!("{arg0}: unexpected error 3: {errno:?}");
              EXIT_FAILURE
            },
          }
        },
        Err(errno) => {
          println!("{arg0}: unexpected error 4: {errno:?}");
          EXIT_FAILURE
        },
      }
    },
  }
}

pub fn rm(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
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
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname, recurse }) => {
      let vinode = match kernel.vfs.lookup_path(&pathname) {
        Ok(vinode) => vinode,
        Err(Errno::ENOENT(_)) => {
          println!("{arg0}: cannot remove '{pathname}': No such file or directory");
          return EXIT_ENOENT;
        },
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: '{pathname}': Permission denied");
          return EXIT_FAILURE
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      };

      // File case
      if vinode.mode.file_type() != FileModeType::Dir as u8 {
        return match kernel.vfs.remove_file(&pathname) {
          Ok(()) => EXIT_SUCCESS,
          Err(Errno::EACCES(_)) => {
            println!("{arg0}: '{pathname}': Permission denied");
            return EXIT_FAILURE
          },
          Err(errno) => {
            println!("{arg0}: unexpected error: {errno:?}");
            EXIT_FAILURE
          },
        }
      }

      // Directory case
      if !recurse {
        println!("{arg0}: cannot remove '{pathname}': Is a directory");
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
            println!("{arg0}: descending into '({new_pathname})'");
            let exit_status = rm(vec![cloned_arg0, String::from("-r"), new_pathname], kernel);
            if exit_status != EXIT_SUCCESS {
              return exit_status;
            }
          }
          return match kernel.vfs.remove_file(&pathname) {
            Ok(()) => EXIT_SUCCESS,
            Err(Errno::EACCES(_)) => {
              println!("{arg0}: '{pathname}': Permission denied");
              return EXIT_FAILURE
            },
            Err(errno) => {
              println!("{arg0}: unexpected error: {errno:?}");
              EXIT_FAILURE
            },
          } 
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          EXIT_FAILURE
        },
      }
    },
  }
}

pub fn mv(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    source_pathname: String,
    target_pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { source_pathname, target_pathname }) => {
      let arg0 = args.get(0).unwrap().clone();
      cp(vec![arg0.clone(), source_pathname.clone(), target_pathname], kernel);
      rm(vec![arg0.clone(), String::from("-r"), source_pathname.clone()], kernel);
      EXIT_SUCCESS
    },
  }
}

pub fn cp(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    source_pathname: String,
    target_pathname: String,
  }

  let arg0 = args.get(0).unwrap().clone();
  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { source_pathname, target_pathname }) => {
      let source_vinode = match kernel.vfs.lookup_path(&source_pathname) {
        Ok(vinode) => vinode,
        Err(Errno::ENOENT(_)) => {
          println!("{arg0}: {source_pathname}: No such file or directory");
          return EXIT_ENOENT;
        },
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: {source_pathname}: Permission denied");
          return EXIT_FAILURE
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      };

      // Guard for target already existing
      if let Ok(_) = kernel.vfs.lookup_path(&target_pathname) {
        println!("{arg0}: {target_pathname}: Already exists");
        return EXIT_FAILURE;
      }

      println!("{arg0}: source_pathname is {source_pathname} source_vinode.mode is {:03b}", source_vinode.mode.file_type());

      // Main part - base file case or recurse
      if source_vinode.mode.file_type() == FileModeType::File as u8 {
        println!("{arg0}: file case");
        let source_bytes = kernel.vfs.read_file(&source_pathname, AddressSize::MAX).unwrap();
        kernel.vfs.create_file(&target_pathname).unwrap();
        kernel.vfs.write_file(&target_pathname, &source_bytes).unwrap();
        EXIT_SUCCESS
      } else {
        println!("{arg0}: dir case (creating dir: {target_pathname})");
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
          println!("{arg0}: descending into '({source_pathname})'");
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
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
    text: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    },
    Ok(BinArgs { pathname, text }) => {
      let bytes = text.as_bytes();
      match kernel.vfs.write_file(&pathname, bytes) {
        Ok(_) => EXIT_SUCCESS,
        Err(Errno::ENOENT(_)) => {
          println!("{arg0}: {pathname}: No such file or directory");
          return EXIT_ENOENT;
        },
        Err(Errno::EISDIR(_)) => {
          println!("{arg0}: {pathname}: Is a directory");
          return EXIT_FAILURE;
        },
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: '{pathname}': Permission denied");
          return EXIT_FAILURE
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      }
    },
  }
}

pub fn ed(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: parse error: {message}");
      1
    },
    Ok(BinArgs { pathname }) => {
      // Read file
      let bytes = match kernel.vfs.read_file(&pathname, AddressSize::MAX) {
        Ok(bytes) => bytes,
        Err(Errno::ENOENT(_)) => {
          println!("{arg0}: {pathname}: No such file or directory");
          return EXIT_ENOENT;
        },
        Err(Errno::EISDIR(_)) => {
          println!("{arg0}: {pathname}: Is a directory");
          return EXIT_FAILURE;
        },
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: '{pathname}': Permission denied");
          return EXIT_FAILURE
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
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
          println!("{arg0}: error while creating platform-provided temp file: {message:#?}");
          return EXIT_FAILURE;
        },
      }
      

      if let Err(message) = Command::new(editor)
          .arg(&file_path)
          .status() 
      {
        println!("{arg0}: error while opening platform-provided editor: {message:#?}");
        return EXIT_FAILURE;
      }

      let mut edited_bytes = Vec::new();

      match File::open(&file_path)
        .and_then(|mut file| file.read_to_end(&mut edited_bytes)) 
      {
        Ok(_) => {
        },
        Err(message) => {
          println!("{arg0}: error while reading back edited platform-provided temp file: {message:#?}");
          return EXIT_FAILURE;
        },
      }

      // Write file back
      return match kernel.vfs.write_file(&pathname, &edited_bytes) {
        Ok(_) => EXIT_SUCCESS,
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: '{pathname}': Permission denied");
          return EXIT_FAILURE
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          EXIT_FAILURE
        },
      }
    },
  }
}

pub fn chmod(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    mode: String,
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname, mode: new_mode_string }) => {
      let old_mode = match kernel.vfs.lookup_path(&pathname) {
        Ok(vinode) => vinode.mode,
        Err(Errno::ENOENT(_)) => {
          println!("{arg0}: {pathname}: No such file or directory");
          return EXIT_ENOENT;
        },
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: '{pathname}': Permission denied");
          return EXIT_FAILURE
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      };

      if !Regex::new("^[0-7]{3}$")
        .unwrap()
        .is_match(&new_mode_string)
        .unwrap()
      {
        println!("{arg0}: invalid mode: '{new_mode_string}'");
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
        Err(Errno::EPERM(_)) => {
          println!("{arg0}: {pathname}: Operation not permitted");
          return EXIT_FAILURE
        },
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: '{pathname}': Permission denied");
          return EXIT_FAILURE
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          return EXIT_FAILURE;
        },
      }
    },
  }
}

pub fn chown(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    new_owners_string: String,
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname, new_owners_string }) => {
      let VINode {
        uid,
        gid,
        ..
      } = match kernel.vfs.lookup_path(&pathname) {
        Ok(vinode) => vinode,
        Err(Errno::EPERM(_)) => {
          println!("{arg0}: changing owner of '{pathname}': Operation not permitted");
          return EXIT_FAILURE
        },
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: '{pathname}': Permission denied");
          return EXIT_FAILURE
        },
        Err(Errno::ENOENT(_)) => {
          println!("{arg0}: {pathname}: No such file or directory");
          return EXIT_ENOENT;
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
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
        println!("{arg0}: invalid user: '{new_owners_string}'");
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
        println!("{arg0}: invalid group: '{new_owners_string}'");
        return EXIT_FAILURE;
      };

      match kernel.vfs.change_owners(&pathname, uid, gid) {
        Ok(_) => EXIT_SUCCESS,
        Err(Errno::EPERM(_)) => {
          println!("{arg0}: changing owner of '{pathname}': Operation not permitted");
          return EXIT_FAILURE
        },
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: '{pathname}': Permission denied");
          return EXIT_FAILURE
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          EXIT_FAILURE
        },
      }
    },
  }
}

// System related stuff
pub fn uname(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { }) => {
      println!("Eunix");
      EXIT_SUCCESS
    },
  }
}

pub fn lsblk(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  let device_table = kernel.devices();
  let mount_points = &kernel.vfs.mount_points;
  println!("{device_table:#?}");
  println!("mount_points: {mount_points:#?}");
  EXIT_SUCCESS
}

pub fn passwd(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    #[clap(short, long, takes_value = false)]
    update: bool,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { update }) => {
      if update && let Err(errno) = kernel.update_uid_gid_maps() {
        println!("{arg0}: cannot update '{PASSWD_PATH}': {errno:?}");
        return EXIT_FAILURE;
      }

      EXIT_SUCCESS
    }
  }
}

pub fn dumpe5fs(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      // let (mount_point, internal_path) = kernel.vfs.match_mount_point(&pathname).unwrap();
      // let mounted_fs = kernel.vfs.mount_points.get_mut(&mount_point).expect("{arg0}::lookup_path: we know that mount_point exist");  
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
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    #[clap(short = 't', long, default_value_t = FilesystemType::e5fs)]
    filesystem_type: FilesystemType,

    source: String,
    target: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: error: {message}");
      1
    }
    Ok(BinArgs {
      filesystem_type,
      source,
      target,
    }) => match kernel.mount(&source, &target, filesystem_type) {
      Ok(_) => 0,
      Err(Errno::EPERM(_)) => {
        println!("{arg0}: unable to mount: Operation not permitted");
        return EXIT_FAILURE
      },
      Err(Errno::EINVAL(message)) => {
        println!("{arg0}: error: {message}");
        1
      }
      Err(_) => unreachable!(),
    },
  }
}

// User related stuff

pub fn id(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
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
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs {}) => {
      let user_name = kernel
        .uid_map
        .get(&kernel.current_uid)
        .unwrap_or(&format!("<no name>({})", kernel.current_uid))
        .clone();

      println!("{user_name}");

      EXIT_SUCCESS
    },
  }
}

pub fn su(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    user: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { user }) => {
      if let Some(uid) = kernel
        .uid_map
        .iter()
        .find(|(_, name)| user == **name)
        .map(|(id, _)| *id)
      {
        let bytes = match kernel.vfs.read_file("/etc/passwd", AddressSize::MAX) {
          Ok(bytes) => bytes,
          Err(Errno::EACCES(_)) => {
            println!("{arg0}: Permission denied");
            return EXIT_FAILURE
          },
          Err(errno) => {
            println!("{arg0}: unexpected error: {errno:?}");
            return EXIT_FAILURE
          },
        };
        let Passwd { name, password, uid, gid, comment, home, shell } = match Passwd::parse_passwds(&String::from_utf8(bytes).unwrap())
          .into_iter()
          .find(|p| p.name == user)
        {
          Some(gid) => gid,
          None => {
            println!("{arg0}: user '{user}' does not exist in /etc/passwd");
            return EXIT_FAILURE;
          },
        };

        // Always switch to user if run as root
        if kernel.current_uid != ROOT_UID {
          let mut input_password = String::new();

          // Read password from user
          print!("Password: ");
          stdout().flush().unwrap();
          stdin().read_line(&mut input_password).unwrap();

          if hex::encode(Sha256::digest(input_password)) != password {
            println!("{arg0}: Authentication failure");
            return EXIT_FAILURE;
          }
        }
        kernel.current_gid = gid;
        kernel.current_uid = uid;
        kernel.update_vfs_current_uid_gid();
        EXIT_SUCCESS
      } else {
        println!("{arg0}: user '{user}' does not exist; you might want to reread /etc/passwd by typing 'passwd -u'");
        EXIT_FAILURE
      }
    },
  }
}

pub fn useradd(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    #[clap(short = 'g', long)]
    primary_group: String,

    #[clap(short = 'c', long, default_value = "")]
    comment: String,

    #[clap(short = 'm', long, default_value = "")]
    home: String,

    #[clap(short = 'G', long, multiple_occurrences = true)]
    supplementary_groups: Vec<String>,

    #[clap(short = 's', long, multiple_occurrences = true, default_value = "")]
    shell: String,

    name: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      EXIT_FAILURE
    }
    Ok(BinArgs { name, primary_group, comment, home, supplementary_groups, shell }) => {
      if kernel.current_uid != ROOT_UID {
        println!("{arg0}: creating user: Operation not permitted");
        return EXIT_FAILURE;
      }

      let bytes = match kernel.vfs.read_file("/etc/passwd", AddressSize::MAX) {
        Ok(bytes) => bytes,
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: Permission denied");
          return EXIT_FAILURE
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          return EXIT_FAILURE
        },
      };
      let contents = String::from_utf8(bytes).unwrap();
      let mut passwds = Passwd::parse_passwds(&contents);

      let mut unclaimed_uids = (1..NOBODY_UID)
        .filter(|uid| !passwds.iter().map(|p| p.uid).contains(uid));

      // Guard for user already existing
      if passwds.iter().map(|p| &p.name).contains(&name) {
        println!("{arg0}: '{name}': User already exists");
        return EXIT_FAILURE;
      }

      let mut password_one = String::new();
      let mut password_two = String::new();

      // Read password from user
      print!("New password: ");
      stdout().flush().unwrap();
      stdin().read_line(&mut password_one).unwrap();
      print!("Retype password: ");
      stdout().flush().unwrap();
      stdin().read_line(&mut password_two).unwrap();

      if password_one != password_two {
        println!("{arg0}: Passwords do not match");
        return EXIT_FAILURE;
      }

      let password_hash = hex::encode(Sha256::digest(&password_one.as_bytes()));

      let new_uid = unclaimed_uids.next().unwrap();
      let new_gid = match kernel
        .gid_map
        .iter()
        .find(|(_, name)| **name == primary_group)
        .map(|(gid, _)| *gid)
      {
        Some(gid) => gid,
        None => {
          println!("{arg0}: group '{primary_group}' does not exist");
          return EXIT_FAILURE
        },
      };
      passwds.push(Passwd {
        name: name.clone(),
        password: password_hash,
        uid: new_uid,
        gid: new_gid,
        comment,
        home: home.clone(),
        shell,
      });

      let serialized = Passwd::serialize_passwds(&passwds);

      // Guard for home dir creation
      if home != "" {
        match kernel.vfs.create_dir(&home) {
          Ok(_) => (),
          Err(Errno::EEXIST(_)) => {
            println!("{arg0}: creating home dir '{home}': Already exists");
            return EXIT_FAILURE
          },
          Err(Errno::EACCES(_)) => {
            println!("{arg0}: creating home dir '{home}': Permission denied");
            return EXIT_FAILURE
          },
          Err(errno) => {
            println!("{arg0}: unexpected error: {errno:?}");
            return EXIT_FAILURE
          },
        };
        kernel.vfs.change_owners(&home, new_uid, new_gid).unwrap();
      }
      
      // Write /etc/passwd
      match kernel.vfs.write_file(PASSWD_PATH, serialized.as_bytes()) {
        Ok(_) => (),
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: Permission denied");
          return EXIT_FAILURE
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          return EXIT_FAILURE
        },
      };

      if let Err(errno) = kernel.update_uid_gid_maps() {
        println!("{arg0}: cannot update '{PASSWD_PATH}': {errno:?}");
      }

      EXIT_SUCCESS
    },
  }
}

pub fn usermod(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    name: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { name }) => {
      EXIT_SUCCESS
    },
  }
}

pub fn userdel(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    name: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { name }) => {
      if kernel.current_uid != ROOT_UID {
        println!("{arg0}: deleting user: Operation not permitted");
        return EXIT_FAILURE;
      }
      let bytes = match kernel.vfs.read_file("/etc/passwd", AddressSize::MAX) {
        Ok(bytes) => bytes,
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: Permission denied");
          return EXIT_FAILURE
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          return EXIT_FAILURE
        },
      };
      let contents = String::from_utf8(bytes).unwrap();
      let passwds = Passwd::parse_passwds(&contents);

      // Guard for user not existing
      if !passwds.iter().map(|p| &p.name).contains(&name) {
        println!("{arg0}: user '{name}' does not exist");
        return EXIT_FAILURE;
      }

      let passwds: Vec<Passwd> = passwds
        .into_iter()
        .filter(|passwd| passwd.name != name)
        .collect();

      let serialized = Passwd::serialize_passwds(&passwds);

      match kernel.vfs.write_file("/etc/passwd", serialized.as_bytes()) {
        Ok(_) => (),
        Err(Errno::EACCES(_)) => {
          println!("{arg0}: Permission denied");
          return EXIT_FAILURE
        },
        Err(errno) => {
          println!("{arg0}: unexpected error: {errno:?}");
          return EXIT_FAILURE
        },
      }

      if let Err(errno) = kernel.update_uid_gid_maps() {
        println!("{arg0}: cannot update '{PASSWD_PATH}': {errno:?}");
      }

      EXIT_SUCCESS
    },
  }
}

pub fn groupmod(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    }
    Ok(BinArgs { pathname }) => {
      EXIT_SUCCESS
    },
  }
}

pub fn groupdel(args: Args, kernel: &mut Kernel) -> AddressSize {
  let arg0 = args.get(0).unwrap().clone();
  #[derive(Debug, Parser)]
  struct BinArgs {
    pathname: String,
  }

  match BinArgs::try_parse_from(args.iter()) {
    Err(message) => {
      println!("{arg0}: invalid arguments: {message}");
      1
    },
    Ok(BinArgs { pathname }) => {
      EXIT_SUCCESS    
    },
  }
}

// vim:ts=2 sw=2
