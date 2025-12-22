//! Macros for error handling in unit fns (generally Tauri callbacks).

macro_rules! fatal {
    ($result:expr) => {
        match $result {
            Ok(_) => (),
            Err(err) => {
                log::error!("{}: {:#}", stringify!($result), err);
                panic!("{}: {:#}", stringify!($result), err);
            }
        }
    };
}

macro_rules! nonfatal {
    ($result:expr) => {
        match $result {
            Ok(x) => x,
            Err(err) => {
                log::error!("{}: {:#}", stringify!($result), err);
                return;
            }
        }
    };
}

#[allow(dead_code, unused_macros)]
macro_rules! optional {
    ($result:expr) => {
        match $result {
            Ok(_) => (),
            Err(err) => {
                log::warn!("{}: {:#}", stringify!($result), err);
            }
        }
    };
}

pub(crate) use fatal;
pub(crate) use nonfatal;
#[allow(unused_imports)]
pub(crate) use optional;
