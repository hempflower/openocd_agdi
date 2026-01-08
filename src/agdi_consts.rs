#![allow(dead_code)]
// AGDI constant definitions
/// AGDI Init item

pub const AG_INITFEATURES: u16      = 0x0100;
pub const AG_GETFEATURE: u16  = 0x0200;
pub const AG_INITITEM: u16      = 0x0300;
/// AGDI Init Callback
pub const AG_INITCALLBACK: u16  = 0x0012;
pub const AG_INITFLASHLOAD: u16 = 0x0013;
pub const AG_STARTFLASHLOAD: u16 = 0x0014;


pub const AG_OK: u32            = 0;
pub const AG_ERR_GENERIC: u32   = 1;
pub const AG_NOACCESS: u32     = 1;


// Callback codes
pub const AG_CB_PROGRESS: u32 = 2;
pub const AG_CB_GETFLASHPARAM: u32 = 15;

// Progress job codes
pub const PROGRESS_INIT: i32 = 1;
pub const PROGRESS_KILL: i32 = 2;
pub const PROGRESS_SETPOS: i32 = 3;