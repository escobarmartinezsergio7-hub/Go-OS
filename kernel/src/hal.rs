#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use core::arch::asm;

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
#[inline(always)]
fn unsupported(op: &str) -> ! {
    panic!("hal::{op} is only supported on x86/x86_64 targets")
}

#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let _ = (port, value);
        unsupported("outb");
    }
}

#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let mut value: u8;
        asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
        asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
        value
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let _ = port;
        unsupported("inb")
    }
}

#[inline]
pub unsafe fn inw(port: u16) -> u16 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let mut value: u16;
        asm!("in ax, dx", in("dx") port, out("ax") value, options(nomem, nostack, preserves_flags));
        value
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let _ = port;
        unsupported("inw")
    }
}

#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let mut value: u32;
        asm!("in eax, dx", in("dx") port, out("eax") value, options(nomem, nostack, preserves_flags));
        value
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let _ = port;
        unsupported("inl")
    }
}

#[inline]
pub unsafe fn outw(port: u16, value: u16) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags));
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let _ = (port, value);
        unsupported("outw");
    }
}

#[inline]
pub unsafe fn outl(port: u16, value: u32) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags));
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let _ = (port, value);
        unsupported("outl");
    }
}

#[inline]
pub unsafe fn rdmsr(msr: u32) -> u64 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let lo: u32;
        let hi: u32;
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack, preserves_flags)
        );
        ((hi as u64) << 32) | lo as u64
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let _ = msr;
        unsupported("rdmsr")
    }
}

#[inline]
pub unsafe fn wrmsr(msr: u32, value: u64) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let lo = value as u32;
        let hi = (value >> 32) as u32;
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") lo,
            in("edx") hi,
            options(nomem, nostack, preserves_flags)
        );
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let _ = (msr, value);
        unsupported("wrmsr");
    }
}

#[inline]
pub fn cli() {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        asm!("cli", options(nomem, nostack, preserves_flags))
    };
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    unsupported("cli");
}

#[inline]
pub fn sti() {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        asm!("sti", options(nomem, nostack, preserves_flags))
    };
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    unsupported("sti");
}

#[inline]
pub fn hlt() {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        asm!("hlt", options(nomem, nostack))
    };
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    unsupported("hlt");
}

#[inline]
pub fn pause() {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        asm!("pause", options(nomem, nostack, preserves_flags))
    };
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    unsupported("pause");
}
