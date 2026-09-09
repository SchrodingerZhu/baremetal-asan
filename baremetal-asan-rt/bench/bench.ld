ENTRY(_start)
MEMORY {
    FLASH (rx) : ORIGIN = 0x02000000, LENGTH = 1M
    RAM (rwx) : ORIGIN = 0x22000000, LENGTH = 0x1A0000
}
SECTIONS {
    .vectors : { KEEP(*(.vectors)) } > FLASH
    .text : { *(.text*) *(.rodata*) } > FLASH
    .ARM.exidx : { *(.ARM.exidx*) } > FLASH
    .data : { __data_start = .; *(.data*) __data_end = .; } > RAM AT> FLASH
    __data_source = LOADADDR(.data);
    .bss (NOLOAD) : { __bss_start = .; *(.bss*) *(COMMON) __bss_end = .; } > RAM
    __stack = ORIGIN(RAM) + LENGTH(RAM);
    /DISCARD/ : { *(.comment) *(.note*) }
}
