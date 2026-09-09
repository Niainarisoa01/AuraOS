/* AuraOS Kernel Linker Script */
/* Places kernel at 1 MiB and ensures no two LOAD segments share a page. */
/* This prevents the bootloader's PageAlreadyMapped panic. */

ENTRY(_start)

SECTIONS
{
    /* Place kernel at 1 MiB virtual address */
    . = 1M;

    .text : ALIGN(4K)
    {
        *(.text .text.*)
    }

    . = ALIGN(4K);

    .rodata : ALIGN(4K)
    {
        *(.rodata .rodata.*)
    }

    . = ALIGN(4K);

    .data : ALIGN(4K)
    {
        *(.data .data.*)
        *(.got .got.*)
        *(.dynamic)
    }

    . = ALIGN(4K);

    .bss : ALIGN(4K)
    {
        *(.bss .bss.*)
    }

    /* Discard unneeded sections */
    /DISCARD/ :
    {
        *(.eh_frame)
        *(.eh_frame_hdr)
        *(.comment)
    }
}
