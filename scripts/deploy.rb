require 'fileutils'

# Rutas (ajusta según tu estructura en la captura)
SOURCE_EFI = "kernel/target/x86_64-unknown-uefi/debug/redux_kernel.efi"
USB_MOUNT_POINT = "/Volumes/BOOTOS" # Punto de montaje en Mac

def install_to_usb
  if Dir.exist?(USB_MOUNT_POINT)
    target_dir = "#{USB_MOUNT_POINT}/EFI/BOOT"
    FileUtils.mkdir_p(target_dir)
    
    # Copiar y renombrar al estándar UEFI
    FileUtils.cp(SOURCE_EFI, "#{target_dir}/BOOTX64.EFI")
    
    puts "✅ Kernel cargado en la USB. ¡Ya puedes bootear!"
  else
    puts "❌ Error: No se encuentra la USB montada en #{USB_MOUNT_POINT}"
  end
end

install_to_usb