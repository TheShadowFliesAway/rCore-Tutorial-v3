//! Trap handling functionality
//!
//! For rCore, we have a single trap entry point, namely `__alltraps`. At
//! initialization in [`init()`], we set the `stvec` CSR to point to it.
//!
//! All traps go through `__alltraps`, which is defined in `trap.S`. The
//! assembly language code does just enough work restore the kernel space
//! context, ensuring that Rust code safely runs, and transfers control to
//! [`trap_handler()`].
//!
//! It then calls different functionality based on what exactly the exception
//! was. For example, timer interrupts trigger task preemption, and syscalls go
//! to [`syscall()`].

// 声明子模块，父亲模块可以直接使用子模块中的东西，也就是trapContext
mod context;

use crate::batch::run_next_app;
use crate::syscall::syscall;
use core::arch::global_asm;
use riscv::register::{
    mtvec::TrapMode,
    scause::{self, Exception, Trap},
    stval, stvec,
};

global_asm!(include_str!("trap.S"));

/// initialize CSR `stvec` as the entry of `__alltraps`
pub fn init() {
    extern "C" {
        fn __alltraps(); //引入这个函数
    }
    unsafe {
        // stvec是一个64位的CSR，在中断使能的情况下，保存了中断处理的入口地址
        // 两个字段：MODE 位于 [1:0]，长度为 2 bits； BASE 位于 [63:2]，长度为 62 bits；
        // MODE字段为0的时候，stvec 被设置为 Direct 模式，此时进入 S 模式的 Trap 无论原因如何，处理 Trap 的入口地址都是 BASE<<2（异常处理入口），还可以设置为vectored（向量模式？）
        // trap.init()做得事情：设置这个函数为异常处理函数。具体而言：异常处理函数保护现场(用户栈)，再将现场作为参数传递给真正的异常处理函数trap_handler
        stvec::write(__alltraps as usize, TrapMode::Direct);
    }
}

#[no_mangle]
/// handle an interrupt, exception, or system call from user space
/// 汇编那边调用的，传过来的参数是sp，栈指针的值（此时代表的是内核的栈），这个栈里面存储的全部都是寄存器的值
/// 实际上来讲，是将栈指针强转为TrapContext指针，之后的访问按照TrapContext的结构解析，因为他们构造是一样的
pub fn trap_handler(cx: &mut TrapContext) -> &mut TrapContext {
    let scause = scause::read(); // get trap cause
    let stval = stval::read(); // get extra value
    match scause.cause() {
        Trap::Exception(Exception::UserEnvCall) => {
            cx.sepc += 4; // sepc 记录的是发生trap的那条指令的地址（硬件记录的，硬件知道发生了trap），这里trap的类型是系统调用，因此从恢复之后应该执行下一条指令，所以sepc+4(RISCV指令长都是32位，六种基本指令格式)
            cx.x[10] = syscall(cx.x[17], [cx.x[10], cx.x[11], cx.x[12]]) as usize;
        }
        Trap::Exception(Exception::StoreFault) | Trap::Exception(Exception::StorePageFault) => {
            println!("[kernel] PageFault in application, kernel killed it.");
            run_next_app();
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            println!("[kernel] IllegalInstruction in application, kernel killed it.");
            run_next_app();
        }
        _ => {
            panic!(
                "Unsupported trap {:?}, stval = {:#x}!",
                scause.cause(),
                stval
            );
        }
    }
    cx
}

pub use context::TrapContext;
