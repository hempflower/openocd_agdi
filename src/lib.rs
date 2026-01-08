mod agdi_consts;
mod agdi_impl;
mod gdb_client;

use core::ffi::c_void;

use crate::{agdi_consts::AG_NOACCESS, agdi_impl::{AG_Bps, GADR, GVAL}};

#[unsafe(no_mangle)]
pub extern "C" fn AG_Init(n_code: u16, vp: *mut c_void) -> u32 {
    agdi_impl::get_agdi().lock().unwrap().init(n_code, vp)
}

// Keil 会检查这些函数的存在，即使不调用它们

#[unsafe(no_mangle)]
pub extern "C" fn AG_MemAtt(_n_code: u16, _n_attr: u32, _pa: *mut GADR) -> u32 {
    AG_NOACCESS
}

#[unsafe(no_mangle)]
pub extern "C" fn AG_BpInfo(_n_code: u16, _vp: *mut c_void) -> u32 {
    AG_NOACCESS
}

#[unsafe(no_mangle)]
pub extern "C" fn AG_BreakFunc(_n_code: u16, _n1: u16, _pa: *mut GADR, _pb: *mut AG_Bps) -> u32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn AG_GoStep(_n_code: u16, _n_steps: u32, _pa: *mut GADR) -> u32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn AG_Serial(_n_code: u16, _n_serial_no: u32, _n_many: u32, _vp: *mut c_void) -> u32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn AG_MemAcc(_n_code: u16, _pb: *mut u8, _pa: *mut GADR, _n_many: u32) -> u32 {
    AG_NOACCESS
}

#[unsafe(no_mangle)]
pub extern "C" fn AG_RegAcc(_n_code: u16, _n_reg:u32,_pv: *mut GVAL) -> u32 {
    AG_NOACCESS
}

#[unsafe(no_mangle)]
pub extern "C" fn AG_AllReg(_n_code: u16, _pr: *mut c_void) -> u32 {
    AG_NOACCESS
}

#[unsafe(no_mangle)]
pub extern "C" fn AG_HistFunc(_n_code: u32,_index:i32,_dir:i32, _vp: *mut c_void) -> u32 {
    AG_NOACCESS
}

#[unsafe(no_mangle)]
pub extern "C" fn DllUv3Cap(n_code: u32, _vp: *mut c_void) -> i32 {
    agdi_impl::get_agdi().lock().unwrap().dll_uv3_cap(n_code, _vp)
}

#[unsafe(no_mangle)]
pub extern "C" fn EnumUvARM7(_vp: *mut c_void, n_code: u16) -> u32 {
    agdi_impl::get_agdi().lock().unwrap().enum_uv_arm7(n_code)
}
