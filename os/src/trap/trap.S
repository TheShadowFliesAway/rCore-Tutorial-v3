# 启用一种替代的宏机制，允许使用.macro和.endm来定义自己的汇编宏
.altmacro
# 定义宏SAVE_GP，它接受一个参数n，表示寄存器编号。
.macro SAVE_GP n
    # sd 全称为 store dobleword,存储64位数据。\n表示参数n的值，\ 是占位符，sp指的是栈指针寄存器
    # 作用：将n号寄存器的值存储到栈中的 sp + n*8 的地址上。为什么要*8，因为存储的是dw，也就是64bit，8个字节，所以0号寄存器存储在sp，1号寄存器存储在 sp+8 以此类推
    sd x\n, \n*8(sp)
# 宏定义结束标志
.endm

#定义宏 LOAD_GP
.macro LOAD_GP n
    #ld 全称为 load doubleword，正好和上面的操作相反
    #作用： 从sp+n*8的位置加载8个字节（dw）到寄存器n中
    ld x\n, \n*8(sp)
.endm

    #指定后续的部分属于 代码部分(.text)，也就是代码存储区
    .section .text
    # .globl 告诉链接器这些符号是全局的，允许其他文件或者模块访问他们。通常用于函数名或者全局变量
    # 这两个下面可以看到都是函数名
    .globl __alltraps
    .globl __restore
    # 要求对齐，对齐到2的2次方字节，即4字节对齐
    .align 2

# 这个函数主要是用于保存异常调用前的状态
__alltraps:
    # csrrw,其实应该分开写，csr rw，就是操作csr寄存器，这个指令的格式为 csrrw rd csr rs,将csr中的值写入rd,在将rs中的值写入csr
    # 这里rd和rs一样的，所以实现的效果是将sp 和 sscratch这两个寄存器的值进行交换
    # 逻辑上来讲，交换之后sp指向内核栈，sscratch指向用户栈，交换之前相反
    csrrw sp, sscratch, sp
    # now sp->kernel stack, sscratch->user stack

    # allocate a TrapContext on kernel stack
    # i指的是 immediate，也就是立即数。格式为 addi rd rs imm。也就是 sp - 34*8 然后再将值放入sp中。什么要减法？因为存储的时候是向高地址增长的，比如sp sp+8 sp+16
    # 按照 TrapContext 结构体的内存布局，基于内核栈的位置（sp所指地址）来从低地址到高地址分别按顺序放置 x0~x31这些通用寄存器，最后是 sstatus 和 sepc 。
    addi sp, sp, -34*8

    # save general-purpose registers
    #使用了存储宏
    sd x1, 1*8(sp)

    # skip sp(x2), we will save it later
    #使用了存储宏
    sd x3, 3*8(sp)

    # skip tp(x4), application does not use it
    # save x5~x31
    #定义一个变量 值为5
    .set n, 5
    # 重复执行指定的代码块，执行27次，也就是 5~31,保存5到31号寄存器的值
    .rept 27
        SAVE_GP %n
        .set n, n+1
    .endr

    # we can use t0/t1/t2 freely, because they were saved on kernel stack
    # csrr(ead) 读取sstatus（包含用户态的状态信息，比如权限级别、中断使能）到寄存器t1里面
    csrr t0, sstatus
    # 读取spec（存储用户态发生异常或者中断的地址）到t1，稍后需要恢复
    csrr t1, sepc

    # 存储起来
    # 为什么不直接 sd sstatus, 32*8(sp)？ 因为sstatus是特殊寄存器，也就是csr，需要使用csr指令才能操作，而不能像普通的寄存器那样直接访问，主要是为了保护
    sd t0, 32*8(sp)
    # 存储起来
    sd t1, 33*8(sp)

    # read user stack from sscratch and save it on the kernel stack
    # 把用户栈指针的值读取到t2当中去
    csrr t2, sscratch

    # 再存储到内核栈里面去
    sd t2, 2*8(sp)

    # set input argument of trap_handler(cx: &mut TrapContext)
    #将sp的值传递给a0，a0是RISC-V中函数调用的第一个参数寄存器，这里是将内核栈指针传递给trap_handler这个函数
    mv a0, sp
    call trap_handler



#sscratch什么时候指向内核栈的呢？
__restore:
    # case1: start running app by __restore
    # case2: back to U after handling trap

    # 传过来的参数是内核栈指针的值，栈指针寄存器sp 也就是x[2]中，所以现在sp指向的是内核栈
    mv sp, a0
    # now sp->kernel stack(after allocated), sscratch->user stack
    # restore sstatus/sepc
    # 内核栈中存储的一些数据读出来，恢复一些寄存器的值
    ld t0, 32*8(sp)
    ld t1, 33*8(sp)
    ld t2, 2*8(sp)
    # 状态信息，比较重要的是spp，也就是trap之前的的特权级，写回sstatus寄存器，在sret的时候CPU可以切换
    csrw sstatus, t0
    # 断点地址（陷入之前的程序执行到这里，导致陷入的那条指令的地址），断点地址会被恢复到sepc寄存器上
    csrw sepc, t1
    # 用户栈指针，读到了sscratch这个里面
    csrw sscratch, t2

    # restore general-purpuse registers except sp/tp
    # 恢复用户程序执行现场
    ld x1, 1*8(sp)
    ld x3, 3*8(sp)
    .set n, 5
    .rept 27
        LOAD_GP %n
        .set n, n+1
    .endr


    # release TrapContext on kernel stack
    # 释放内核栈的空间
    addi sp, sp, 34*8
    # now sp->kernel stack, sscratch->user stack
    # 将sp指向用户栈，用sscratch记录内核栈的指针
    csrrw sp, sscratch, sp
    #从内核态进行返回
    sret
