use riscv::register::sstatus::{self, Sstatus, SPP};
/// Trap Context
#[repr(C)]
pub struct TrapContext {
    /// general regs[0..31]，32个通用寄存器的值
    pub x: [usize; 32],
    /// CSR sstatus，记录trap之前CPU处在那个特权级别，这个是S特权级别下面最重要的CSR
    pub sstatus: Sstatus,// 第33个 也就是标为32的寄存器
    /// CSR sepc，当trap是一个异常的时候，记录trap发生之前执行的最后一条指令的地址
    pub sepc: usize, //第34个，也就是下标为33的寄存器
}

impl TrapContext {
    /// set stack pointer to x_2 reg (sp)
    pub fn set_sp(&mut self, sp: usize) {
        self.x[2] = sp;
    }
    /// init app context
    /// 初始化app运行的上下文
    pub fn app_init_context(entry: usize, sp: usize) -> Self {
        //这里是读出状态寄存器其值
        let mut sstatus = sstatus::read(); // CSR sstatus，状态寄存器，包括SPP字段，表示在Trap之前CPU处于哪个特权级别
        //这里是将值改动，主要是SPP字段，改成User
        sstatus.set_spp(SPP::User); //previous privilege mode: user mode，然后将SPP设置为User，表示在从内核返回之后CPU会切换到用户态执行
        let mut cx = Self {
            x: [0; 32],
            sstatus,
            sepc: entry, // entry point of app 第34个寄存器的值填入的是断点地址
        };
        cx.set_sp(sp); // app's user stack pointer，设置用户的栈指针
        cx // return initial Trap Context of app
    }
}
