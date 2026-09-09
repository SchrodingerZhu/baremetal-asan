.syntax unified
.thumb
# LLVM-MCA-BEGIN scalar_byte
ldrb r2, [r0], #1
cbnz r2, byte_done
subs r1, #1
bne byte_loop
byte_loop:
byte_done:
# LLVM-MCA-END
# LLVM-MCA-BEGIN scalar_word
ldr r2, [r0]
cbnz r2, word_done
subs r1, #4
adds r0, #4
cmp r1, #3
bhi word_loop
word_loop:
word_done:
# LLVM-MCA-END
# LLVM-MCA-BEGIN mve
vctp.8 r1
vpst
vldrbt.u8 q0, [r0]
vcmp.i8 ne, q0, zr
vmrs r2, p0
cbnz r2, mve_done
subs r1, #16
add.w r0, r0, #16
bhi mve_loop
mve_loop:
mve_done:
# LLVM-MCA-END
