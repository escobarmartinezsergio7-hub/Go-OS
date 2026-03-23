#include <stdint.h>
#include "kernel_api.h"

static volatile uint16_t* const VGA = (uint16_t*)0xB8000;
static uint8_t cursor_row = 0;
static uint8_t cursor_col = 0;
static uint8_t color = 0x0B; // cyan on black

static inline uint16_t vga_entry(char c, uint8_t col) {
    return (uint16_t)c | ((uint16_t)col << 8);
}

static void scroll_if_needed() {
    if (cursor_row < 25) return;

    for (uint32_t y = 1; y < 25; ++y) {
        for (uint32_t x = 0; x < 80; ++x) {
            VGA[(y - 1) * 80 + x] = VGA[y * 80 + x];
        }
    }

    for (uint32_t x = 0; x < 80; ++x) {
        VGA[24 * 80 + x] = vga_entry(' ', color);
    }

    cursor_row = 24;
}

extern "C" void cpp_clear_screen(void) {
    for (uint32_t i = 0; i < 80 * 25; ++i) {
        VGA[i] = vga_entry(' ', color);
    }
    cursor_row = 0;
    cursor_col = 0;
}

extern "C" void cpp_set_color(unsigned char fg, unsigned char bg) {
    color = (bg << 4) | (fg & 0x0F);
}

extern "C" void cpp_putc(char c) {
    if (c == '\n') {
        cursor_col = 0;
        ++cursor_row;
        scroll_if_needed();
        return;
    }

    if (c == '\b') {
        if (cursor_col > 0) {
            --cursor_col;
            VGA[cursor_row * 80 + cursor_col] = vga_entry(' ', color);
        }
        return;
    }

    VGA[cursor_row * 80 + cursor_col] = vga_entry(c, color);
    ++cursor_col;

    if (cursor_col >= 80) {
        cursor_col = 0;
        ++cursor_row;
    }

    scroll_if_needed();
}

extern "C" void cpp_print(const char* s) {
    if (!s) return;
    while (*s) {
        cpp_putc(*s++);
    }
}

extern "C" void cpp_println(const char* s) {
    cpp_print(s);
    cpp_putc('\n');
}

static inline uint8_t inb(uint16_t port) {
    uint8_t ret;
    __asm__ volatile("inb %1, %0" : "=a"(ret) : "Nd"(port));
    return ret;
}

static inline void outb(uint16_t port, uint8_t value) {
    __asm__ volatile("outb %0, %1" : : "a"(value), "Nd"(port));
}

static inline uint32_t inl(uint16_t port) {
    uint32_t ret;
    __asm__ volatile("inl %1, %0" : "=a"(ret) : "Nd"(port));
    return ret;
}

static inline void outl(uint16_t port, uint32_t value) {
    __asm__ volatile("outl %0, %1" : : "a"(value), "Nd"(port));
}

static const char scancode_ascii[128] = {
    0, 27, '1','2','3','4','5','6','7','8','9','0','-','=', '\b', '\t',
    'q','w','e','r','t','y','u','i','o','p','[',']','\n', 0, 'a','s','d',
    'f','g','h','j','k','l',';','\'', '`', 0, '\\','z','x','c','v','b','n',
    'm',',','.','/', 0, '*', 0, ' ',
};

extern "C" char cpp_keyboard_poll(void) {
    if ((inb(0x64) & 0x01) == 0) return 0;

    uint8_t sc = inb(0x60);
    if (sc & 0x80) return 0; // key release

    if (sc < sizeof(scancode_ascii)) {
        return scancode_ascii[sc];
    }
    return 0;
}

static void print_hex_nibble(uint8_t v) {
    const char* hex = "0123456789ABCDEF";
    cpp_putc(hex[v & 0x0F]);
}

static void print_hex8(uint8_t v) {
    print_hex_nibble((v >> 4) & 0x0F);
    print_hex_nibble(v & 0x0F);
}

static void print_hex16(uint16_t v) {
    print_hex8((uint8_t)((v >> 8) & 0xFF));
    print_hex8((uint8_t)(v & 0xFF));
}

static uint32_t pci_read(uint8_t bus, uint8_t slot, uint8_t func, uint8_t offset) {
    uint32_t address = (uint32_t)(0x80000000u
        | ((uint32_t)bus << 16)
        | ((uint32_t)slot << 11)
        | ((uint32_t)func << 8)
        | (offset & 0xFC));

    outl(0xCF8, address);
    return inl(0xCFC);
}

extern "C" void cpp_pci_scan_brief(void) {
    cpp_println("PCI scan (first 24 devices):");
    int shown = 0;

    for (uint16_t bus = 0; bus < 256 && shown < 24; ++bus) {
        for (uint8_t slot = 0; slot < 32 && shown < 24; ++slot) {
            uint32_t id = pci_read((uint8_t)bus, slot, 0, 0x00);
            uint16_t vendor = (uint16_t)(id & 0xFFFF);
            if (vendor == 0xFFFF) continue;

            uint16_t device = (uint16_t)((id >> 16) & 0xFFFF);
            uint32_t class_data = pci_read((uint8_t)bus, slot, 0, 0x08);
            uint8_t base_class = (uint8_t)((class_data >> 24) & 0xFF);
            uint8_t sub_class = (uint8_t)((class_data >> 16) & 0xFF);

            cpp_print("bus 0x");
            print_hex8((uint8_t)bus);
            cpp_print(" slot 0x");
            print_hex8(slot);
            cpp_print(" vendor 0x");
            print_hex16(vendor);
            cpp_print(" device 0x");
            print_hex16(device);
            cpp_print(" class 0x");
            print_hex8(base_class);
            cpp_print(" sub 0x");
            print_hex8(sub_class);
            cpp_putc('\n');

            ++shown;
        }
    }

    if (shown == 0) {
        cpp_println("No PCI devices found.");
    }
}

extern "C" void cpp_reboot(void) {
    cpp_println("Rebooting...");

    while (inb(0x64) & 0x02) {}
    outb(0x64, 0xFE);

    for (;;) {
        __asm__ volatile("hlt");
    }
}
