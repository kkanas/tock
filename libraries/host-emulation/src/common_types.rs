use zerocopy::{AsBytes, FromBytes, Unaligned};

#[repr(C, packed)]
#[derive(Unaligned)]
#[derive(AsBytes, FromBytes)]
#[derive(Default, Debug, Copy, Clone)]
pub struct Syscall {
    pub syscall_number: usize,
    pub args: [usize; 4],
}

#[repr(C, packed)]
#[derive(Unaligned)]
#[derive(AsBytes, FromBytes)]
#[derive(Default, Debug, Copy, Clone)]
pub struct Callback {
    pc: usize,
    args: [usize; 4],
}

#[repr(C, packed)]
#[derive(Unaligned)]
#[derive(AsBytes, FromBytes)]
#[derive(Default, Debug, Copy, Clone)]
pub struct KernelReturn {
    ret_val: isize,
    cb: Callback,
}

#[repr(C, packed)]
#[derive(Unaligned)]
#[derive(AsBytes, FromBytes)]
#[derive(Default, Debug, Copy, Clone)]
pub struct AllowedRegionPreamble {
    address: usize,
    length: usize,
}

impl Syscall {
    pub const fn new(
        syscall_number: usize,
        arg0: usize,
        arg1: usize,
        arg2: usize,
        arg3: usize,
    ) -> Syscall {
        Syscall {
            syscall_number,
            args: [
                arg0,
                arg1,
                arg2,
                arg3,
            ],
        }
    }

    pub const fn new0(syscall_number: usize) -> Syscall {
        Syscall::new1(syscall_number, 0)
    }

    pub const fn new1(syscall_number: usize, arg0: usize) -> Syscall {
        Syscall::new2(syscall_number, arg0, 0)
    }

    pub const fn new2(
        syscall_number: usize,
        arg0: usize,
        arg1: usize,
    ) -> Syscall {
        Syscall::new3(syscall_number, arg0, arg1, 0)
    }

    pub const fn new3(
        syscall_number: usize,
        arg0: usize,
        arg1: usize,
        arg2: usize,
    ) -> Syscall {
        Syscall::new(syscall_number, arg0, arg1, arg2, 0)
    }
}

impl Callback {
    pub const fn new(
        pc: usize,
        arg0: usize,
        arg1: usize,
        arg2: usize,
        arg3: usize,
    ) -> Callback {
        Callback {
            pc,
            args: [
                arg0,
                arg1,
                arg2,
                arg3,
            ]
        }
    }

    pub const fn new0(pc: usize) -> Callback {
        Callback::new1(pc, 0)
    }

    pub const fn new1(pc: usize, arg0: usize) -> Callback {
        Callback::new2(pc, arg0, 0)
    }

    pub const fn new2(pc: usize, arg0: usize, arg1: usize) -> Callback {
        Callback::new3(pc, arg0, arg1, 0)
    }

    pub const fn new3(
        pc: usize,
        arg0: usize,
        arg1: usize,
        arg2: usize,
    ) -> Callback {
        Callback::new(pc, arg0, arg1, arg2, 0)
    }
}

impl KernelReturn {
    const fn new(
        ret_val: isize,
        cb: Callback
    ) -> KernelReturn {
        KernelReturn {
            ret_val,
            cb,
        }
    }

    pub const fn new_ret(ret_val: isize) -> KernelReturn {
        KernelReturn::new(ret_val, Callback::new0(0))
    }

    pub const fn new_cb(cb: Callback) -> KernelReturn {
        KernelReturn::new(0, cb)
    }
}

impl AllowedRegionPreamble {
    pub const fn new(address: usize, length: usize) -> AllowedRegionPreamble {
        AllowedRegionPreamble { address, length }
    }

    pub const fn new_null() -> AllowedRegionPreamble {
        AllowedRegionPreamble::new(0, 0)
    }
}

