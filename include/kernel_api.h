#ifndef REDUX_KERNEL_API_H
#define REDUX_KERNEL_API_H

#ifdef __cplusplus
extern "C" {
#endif

void cpp_clear_screen(void);
void cpp_set_color(unsigned char fg, unsigned char bg);
void cpp_putc(char c);
void cpp_print(const char* s);
void cpp_println(const char* s);
char cpp_keyboard_poll(void);
void cpp_pci_scan_brief(void);
void cpp_reboot(void);

#ifdef __cplusplus
}
#endif

#endif
