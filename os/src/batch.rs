//! batch subsystem

use crate::sbi::shutdown;
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use core::arch::asm;
use lazy_static::*;

const USER_STACK_SIZE: usize = 4096 * 2;
const KERNEL_STACK_SIZE: usize = 4096 * 2;
const MAX_APP_NUM: usize = 16;
const APP_BASE_ADDRESS: usize = 0x80400000;
const APP_SIZE_LIMIT: usize = 0x20000;

//内核的栈，这个属性的含义是
#[repr(align(4096))]
struct KernelStack {
    data: [u8; KERNEL_STACK_SIZE],
}


//用户区代码的栈
//这里4096的含义是：这个类型的实例，也就是UserStack对象在存储的时候其首地址必须是4096的倍数
//#[repr(align(N))] 用来 显式设置类型的对齐方式，其中 N 是一个对齐字节数，必须是 2 的幂，例如 1, 2, 4, 8, 16, 32, ..., 4096 等。
#[repr(align(4096))]
struct UserStack {
    data: [u8; USER_STACK_SIZE],
}

static KERNEL_STACK: KernelStack = KernelStack {
    data: [0; KERNEL_STACK_SIZE],
};
static USER_STACK: UserStack = UserStack {
    data: [0; USER_STACK_SIZE],
};

impl KernelStack {
    // 获取栈顶指针
    fn get_sp(&self) -> usize {
        self.data.as_ptr() as usize + KERNEL_STACK_SIZE
    }
    pub fn push_context(&self, cx: TrapContext) -> &'static mut TrapContext {
        let cx_ptr = (self.get_sp() - core::mem::size_of::<TrapContext>()) as *mut TrapContext;
        unsafe {
            // 是个赋值操作，把传入的cx逐个字节的拷贝到上面的栈空间中
            *cx_ptr = cx;
        }
        unsafe { cx_ptr.as_mut().unwrap() }
    }
}

impl UserStack {
    fn get_sp(&self) -> usize {
        self.data.as_ptr() as usize + USER_STACK_SIZE
    }
}

struct AppManager {
    num_app: usize,
    current_app: usize,
    app_start: [usize; MAX_APP_NUM + 1],
}

impl AppManager {
    pub fn print_app_info(&self) {
        println!("[kernel] num_app = {}", self.num_app);
        for i in 0..self.num_app {
            println!(
                "[kernel] app_{} [{:#x}, {:#x})",
                i,
                self.app_start[i],
                self.app_start[i + 1]
            );
        }
    }

    unsafe fn load_app(&self, app_id: usize) {
        if app_id >= self.num_app {
            println!("All applications completed!");
            shutdown(false);
        }
        println!("[kernel] Loading app_{}", app_id);
        // 把要运行区域的数据先清空
        core::slice::from_raw_parts_mut(APP_BASE_ADDRESS as *mut u8, APP_SIZE_LIMIT).fill(0);

        // 这些程序都被加载到了内存里面，现在需要搬运，从内存中的加载的位置搬运到程序运行的位置，也就是0x80400000，搬运到这里

        //1、标记出程序被加载到的位置
        let app_src = core::slice::from_raw_parts(
            self.app_start[app_id] as *const u8, //起始位置
            self.app_start[app_id + 1] - self.app_start[app_id],//长度
        );
        //2、标记出程序要执行需要被加载到的空间
        let app_dst = core::slice::from_raw_parts_mut(APP_BASE_ADDRESS as *mut u8, app_src.len());

        //3、将1中的数据复制到2中，准备让程序进行执行
        app_dst.copy_from_slice(app_src);
        // Memory fence about fetching the instruction memory
        // It is guaranteed that a subsequent instruction fetch must
        // observes all previous writes to the instruction memory.
        // Therefore, fence.i must be executed after we have loaded
        // the code of the next app into the instruction memory.
        // See also: riscv non-priv spec chapter 3, 'Zifencei' extension.
        // fence.i 就是一条 riscv 的汇编指令，保证两件事：1、cache中的修改写回主存完成（写命中，回写法）2、强制cache刷新
        asm!("fence.i");
    }

    pub fn get_current_app(&self) -> usize {
        self.current_app
    }

    pub fn move_to_next_app(&mut self) {
        self.current_app += 1;
    }
}


// lazy_static! 这个宏提供全局变量运行时的初始化功能！自己初始化麻烦，不自己初始化要使用static mut声明，会衍生出很多unsafe代码，因此使用lazy_static! 可以省很多力
// 就是运行时的初始化全局变量，叫做 延迟初始化
lazy_static! {
    static ref APP_MANAGER: UPSafeCell<AppManager> = unsafe { // ref 不是单独的关键字，而是 static ref 模式的一部分，表示这是一个延迟初始化的全局变量，在第一次访问时才会进行初始化
        UPSafeCell::new({
            extern "C" {
                fn _num_app();
            }
            let num_app_ptr = _num_app as usize as *const usize;
            let num_app = num_app_ptr.read_volatile();
            let mut app_start: [usize; MAX_APP_NUM + 1] = [0; MAX_APP_NUM + 1]; //声明一个数组，名字app_start,元素类型为usize，大小为MAX_APP_NUM+1，全部初始化为0

            // num_app_ptr.add(1) 是对原始指针进行偏移，使其指向数组中的第二个元素（num_app 之前存储的是应用数量）。
            // core::slice::from_raw_parts 函数将原始指针和元素个数转换成一个切片 (&[usize])。app_start_raw 就是一个引用，指向从 num_app_ptr.add(1) 开始的内存区域，长度为 num_app + 1。
            // 这行代码把 num_app_ptr.add(1) 指向的内存区间转换成一个切片，表示从该内存地址开始的 num_app + 1 个 usize 类型的元素，这几个usize的含义是这几个app的起始地址。
            let app_start_raw: &[usize] =
                core::slice::from_raw_parts(num_app_ptr.add(1), num_app + 1);

            // 这行代码将 app_start_raw 中的数据复制到 app_start 数组中。  ..=num_app（省略起始位置的写法，其实就是0），表示 app_start 数组的前 num_app + 1 个元素
            app_start[..=num_app].copy_from_slice(app_start_raw);
            AppManager {
                num_app,
                current_app: 0,
                app_start,
            }
        })
    };
}

/// init batch subsystem
pub fn init() {
    print_app_info();
}

/// print apps info
pub fn print_app_info() {
    // 第一次使用的时候会执行lazy_static!中的代码，进行初始化
    APP_MANAGER.exclusive_access().print_app_info();
}

/// run next app
pub fn run_next_app() -> ! {
    // 获取全局变量的可变引用
    let mut app_manager = APP_MANAGER.exclusive_access();
    let current_app = app_manager.get_current_app();
    // 加载当前的app,并不执行
    unsafe {
        app_manager.load_app(current_app);
    }
    // 改变current_app的值，使其加1
    app_manager.move_to_next_app();
    // 之后不再使用这个全局变量的可变引用，所以要进行删除
    drop(app_manager);
    // before this we have to drop local variables related to resources manually
    // and release the resources
    extern "C" {
        // 引入这个函数
        fn __restore(cx_addr: usize);
    }
    unsafe {
        // 内核初始化已经完成，从内核区返回到用户区准备执行用户代码
        __restore(KERNEL_STACK.push_context(TrapContext::app_init_context(
            APP_BASE_ADDRESS,
            USER_STACK.get_sp(), //设置用户栈，编译时无需关心 sp 的值，栈操作代码由编译器生成。运行时操作系统设置 sp，决定用户程序的栈位置。
        )) as *const _ as usize);
    }
    panic!("Unreachable in batch::run_current_app!");
}
