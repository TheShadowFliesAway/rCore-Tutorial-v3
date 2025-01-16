    .section .text.entry
    .globl _start
_start:
    # la load address的缩写，将一个标签的地址加载到寄存器中
    la sp, boot_stack_top
    call rust_main

    .section .bss.stack
    .globl boot_stack_lower_bound
boot_stack_lower_bound:
    .space 4096 * 16
    .globl boot_stack_top
boot_stack_top: