use crate::agdi_consts::{
    AG_CB_GETFLASHPARAM, AG_CB_PROGRESS, AG_GETFEATURE, AG_INITCALLBACK, AG_INITFLASHLOAD,
    AG_INITITEM, AG_NOACCESS, AG_OK, AG_STARTFLASHLOAD, PROGRESS_INIT, PROGRESS_KILL,
    PROGRESS_SETPOS,
};
use crate::gdb_client::{GdbClient, TcpTransport};
use core::ffi::c_void;
use core::slice;
use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::{Mutex, OnceLock};
use user32::MessageBoxA;
use winapi::winuser::{MB_ICONINFORMATION, MB_OK};

#[repr(C, packed(1))]
#[derive(Default)]
pub struct PgRess {
    pub job: i32,           // PROGRESS_INIT/KILL/SETPOS
    pub pos: i32,           // PROGRESS_SETPOS: position to set
    pub low: i32,           // low percent (normally 0)
    pub hig: i32,           // high percent (normally 100)
    pub label: *mut c_char, // text-label before progress-bar or NULL
    pub ctext: *mut c_char, // Text instead of % display
}

unsafe impl Send for PgRess {}
unsafe impl Sync for PgRess {}

#[repr(C, packed)]
pub struct GADR {
    pub adr: u32,
    pub err_adr: u32,
    pub n_len: u32,
    pub m_space: u16,
}

#[repr(C, packed)]
pub struct AG_Bps {
    // 双向链表指针
    pub next: *mut AG_Bps,
    pub prev: *mut AG_Bps,

    // 位域字段 (总共 7 位，可合并成 u32)
    pub type_enabled_flags: u32,
    // 说明：可以通过掩码访问：
    // type: bits 0-3
    // enabled: bit 4
    // ReCalc: bit 5
    // BytObj: bit 6
    pub adr: u32,                    // 地址或范围
    pub m_space: u32,                // 内存空间
    pub p_v: *mut core::ffi::c_void, // VTR-access breakpoint

    pub tsize: u32,                 // WatchBrk: size of one object
    pub many: u32,                  // WatchBrk: many objects or bytes
    pub acc: u16,                   // 1=Read, 2=Write, 3=ReadWrite
    pub bit_pos: u16,               // currently not used
    pub number: u32,                // BreakPoint-Number
    pub rcount: i32,                // Break is taken when rcount = 1
    pub ocount: i32,                // Original Count
    pub ep: *mut core::ffi::c_void, // conditional expression
    pub cmd: *mut i8,               // Exec-Command (C 字符串)
    pub line: *mut i8,              // Breakpoint-Expression Line for Display
    pub p_f: *mut i8,               // module file name
    pub n_line: u32,                // line number
    pub opc: [u8; 8],               // Opcode-Save Area for Monitors
}

#[repr(C)]
pub union GVAL {
    pub u32: u32,       // 32-Bit unsigned int
    pub i32: i32,       // 32-Bit signed int
    pub ul: u32,        // 32-Bit unsigned long (假设 UL32=u32)
    pub sl: i32,        // 32-Bit signed long (假设 SL32=i32)
    pub uc: u8,         // 8-Bit unsigned char
    pub sc: i8,         // 8-Bit signed char
    pub u16: u16,       // 16-Bit unsigned short int
    pub i16: i16,       // 16-Bit signed short int
    pub u64: u64,       // 64-Bit unsigned int
    pub i64: i64,       // 64-Bit signed int
    pub f32: f32,       // 32-Bit float
    pub f64: f64,       // 64-Bit float
    pub ul2: [u32; 2],  // UL32[2]
    pub sl2: [i32; 2],  // SL32[2]
    pub u16a: [u16; 4], // U16[4]
    pub i16a: [i16; 4], // I16[4]
    pub uc8: [u8; 8],   // UC8[8]
    pub sc8: [i8; 8],   // SC8[8]
    pub p_s: *mut i8,   // SC8*
    pub p_u: *mut u8,   // UC8*
    pub p_w: *mut u16,  // U16*
    pub p_d: *mut u32,  // U32*
}

use core::ffi::c_uchar;

#[repr(C, packed)]
pub struct FlashParm {
    pub start: u32,          // Start-Address
    pub many: u32,           // Number of Bytes
    pub image: *mut c_uchar, // UC8*
    pub act_size: u32,       // total number of bytes

    /// Stop : 1
    /// 用 u32 表示位域所在的整型
    pub stop_and_flags: u32,

    pub res: [u32; 16], // reserved
}

type Pcbf = extern "C" fn(n_code: u32, vp: *mut c_void) -> u32;

fn show_message_box(message: &str, title: &str) {
    let lp_text = CString::new(message).unwrap();
    let lp_caption = CString::new(title).unwrap();
    unsafe {
        MessageBoxA(
            std::ptr::null_mut(),
            lp_text.as_ptr(),
            lp_caption.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

#[inline]
fn align_up(value: u32, align: u32) -> u32 {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}


pub struct Agdi {
    p_callback: Option<Pcbf>,
    gdb_client: GdbClient<TcpTransport>,
}

impl Agdi {
    pub fn new() -> Self {
        Self {
            p_callback: None,
            gdb_client: GdbClient::new(TcpTransport::new("localhost", 3333)),
        }
    }
    pub fn init(&mut self, n_code: u16, _vp: *mut c_void) -> u32 {
        match n_code & 0xFF00 {
            AG_INITITEM => match n_code & 0x00FF {
                AG_INITFLASHLOAD => self.init_flash_load(),
                AG_STARTFLASHLOAD => self.start_flash_load(),
                AG_INITCALLBACK => {
                    // 初始化回调函数指针
                    self.init_callback(_vp);
                    AG_OK
                }
                _ => AG_OK,
            },
            AG_GETFEATURE => 0,
            _ => AG_OK,
        }
    }

    // pub fn mem_att(&self, _n_code: u16, _n_attr: u32, _pa: *mut GADR) -> u32 {
    //     AG_NOACCESS
    // }

    pub fn dll_uv3_cap(&self, n_code: u32, _vp: *mut c_void) -> i32 {
        match n_code {
            1 => 1,
            // ARM target driver
            2 => 7,
            // 支持 Flash 下载
            100 => 1,
            _ => 0,
        }
    }

    pub fn enum_uv_arm7(&self, n_code: u16) -> u32 {
        match n_code {
            2 => 7,
            _ => 0,
        }
    }

    pub fn init_callback(&mut self, vp: *mut c_void) {
        if !vp.is_null() {
            let cb: Pcbf = unsafe { std::mem::transmute(vp) };
            self.p_callback = Some(cb);
        }
    }
    pub fn init_flash_load(&mut self) -> u32 {
        match self.gdb_client.connect() {
            Ok(_) => AG_OK,
            Err(e) => {
                show_message_box(&format!("Failed to connect to GDB server: {}", e), "Error");
                AG_NOACCESS
            }
        }
    }

    fn do_flash_load_internal(&mut self) -> u32 {
        self.progress_bar_init("Loading...");

        // 获取 flash 信息
        let flash_infs = match self.gdb_client.get_flash_info() {
            Ok(i) => i,
            Err(_) => return AG_NOACCESS,
        };

        if flash_infs.len() == 0 {
            return AG_NOACCESS;
        }

        // 获取第一个 flash
        let flash_inf = &flash_infs[0];
        let block_size = flash_inf.blocksize.unwrap_or(1024);

        let mut wrote_bytes = 0;
        let mut pf = unsafe { &mut *self.get_flash_param(core::ptr::null_mut()) };

        // 擦除
        if pf.many != 0 {
            let earse_size = align_up(pf.act_size, block_size as u32);
            match self.gdb_client.flash_erase(pf.start, earse_size) {
                Ok(_) => {}
                Err(_) => return AG_NOACCESS,
            };
        }

        loop {
            if pf.many == 0 {
                break;
            }

            let data: &[u8] =
                unsafe { slice::from_raw_parts(pf.image as *const u8, pf.many as usize) };
            match self.gdb_client.flash_write(pf.start, data, 256) {
                Ok(_) => {}
                Err(_) => return AG_NOACCESS,
            };
            wrote_bytes += pf.many;
            self.progress_bar_setpos((wrote_bytes * 100 / pf.act_size) as i32);
            // get next param
            pf = unsafe { &mut *self.get_flash_param(pf) };
        }
        match self.gdb_client.flash_done() {
            Ok(_) => {}
            Err(_) => return AG_NOACCESS,
        };
        self.progress_bar_kill();

        AG_OK
    }

    pub fn start_flash_load(&mut self) -> u32 {
        let result = self.do_flash_load_internal();
        self.gdb_client.disconnect();
        result
    }

    pub fn get_flash_param(&self, _vp: *mut FlashParm) -> *mut FlashParm {
        let ptr: u32 = self.call_callback(AG_CB_GETFLASHPARAM, _vp as *mut _ as *mut c_void);
        return ptr as usize as *mut FlashParm;
    }

    pub fn call_callback(&self, n_code: u32, vp: *mut c_void) -> u32 {
        if let Some(cb) = self.p_callback {
            // 用消息框显示一下地址
            cb(n_code, vp)
        } else {
            0
        }
    }

    pub fn progress_bar_init(&self, label: &str) -> u32 {
        let c_label = CString::new(label).unwrap();
        let mut pg_ress = PgRess {
            job: PROGRESS_INIT,
            pos: 0,
            low: 0,
            hig: 100,
            label: c_label.into_raw(),
            ctext: std::ptr::null_mut(),
        };

        self.call_callback(AG_CB_PROGRESS, &mut pg_ress as *mut _ as *mut c_void)
    }

    pub fn progress_bar_setpos(&self, pos: i32) -> u32 {
        let mut pg_ress = PgRess {
            job: PROGRESS_SETPOS,
            pos,
            low: 0,
            hig: 100,
            label: std::ptr::null_mut(),
            ctext: std::ptr::null_mut(),
        };
        self.call_callback(AG_CB_PROGRESS, &mut pg_ress as *mut _ as *mut c_void)
    }

    pub fn progress_bar_kill(&self) -> u32 {
        let mut pg_ress = PgRess {
            job: PROGRESS_KILL,
            pos: 0,
            low: 0,
            hig: 100,
            label: std::ptr::null_mut(),
            ctext: std::ptr::null_mut(),
        };
        self.call_callback(AG_CB_PROGRESS, &mut pg_ress as *mut _ as *mut c_void)
    }
}

static AGDI_INSTANCE: OnceLock<Mutex<Agdi>> = OnceLock::new();

pub fn get_agdi() -> &'static Mutex<Agdi> {
    AGDI_INSTANCE.get_or_init(|| Mutex::new(Agdi::new()))
}
