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

pub(crate) use fatal;
pub(crate) use nonfatal;
