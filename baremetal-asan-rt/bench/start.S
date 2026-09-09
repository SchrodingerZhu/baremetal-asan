.syntax unified
.thumb
.section .vectors,"a",%progbits
.balign 1024
.word __stack, _start, fault, fault, fault, fault, fault
.word 0, 0, 0, 0, SVC_Handler, fault, 0, fault, fault

.section .text._start,"ax",%progbits
.global _start
.thumb_func
_start:
    ldr r0, =0xE000ED08
    ldr r1, =0x02000000
    str r1, [r0]
    ldr r0, =__data_start
    ldr r1, =__data_end
    ldr r2, =__data_source
1:  cmp r0, r1
    bhs 2f
    ldrb r3, [r2], #1
    strb r3, [r0], #1
    b 1b
2:  ldr r0, =__bss_start
    ldr r1, =__bss_end
    movs r2, #0
3:  cmp r0, r1
    bhs 4f
    strb r2, [r0], #1
    b 3b
4:  ldr r0, =0xE000ED88
    ldr r1, [r0]
    orr r1, r1, #0x00F00000
    str r1, [r0]
    dsb
    isb
    vmrs r0, fpscr
    orr r0, r0, #0x00040000
    vmsr fpscr, r0
    bl _platform_init
    bl main
    b __llvm_libc_exit

.thumb_func
fault:
    movs r0, #1
    b __llvm_libc_exit

.global SVC_Handler
.thumb_func
SVC_Handler:
    push {r4, lr}
    ldr r0, =isr_addr
    ldr r0, [r0]
    ldr r1, =isr_size
    ldr r1, [r1]
    ldr r2, =isr_fn
    ldr r2, [r2]
    blx r2
    pop {r4, pc}
