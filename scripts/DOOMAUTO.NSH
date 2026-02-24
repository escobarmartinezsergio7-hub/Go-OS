@echo -off

if exist fs0:\EFI\DOOM\DOOMX64.EFI then
  fs0:
  \EFI\DOOM\DOOMX64.EFI
  exit
endif
if exist fs1:\EFI\DOOM\DOOMX64.EFI then
  fs1:
  \EFI\DOOM\DOOMX64.EFI
  exit
endif
if exist fs2:\EFI\DOOM\DOOMX64.EFI then
  fs2:
  \EFI\DOOM\DOOMX64.EFI
  exit
endif
if exist fs3:\EFI\DOOM\DOOMX64.EFI then
  fs3:
  \EFI\DOOM\DOOMX64.EFI
  exit
endif
if exist fs4:\EFI\DOOM\DOOMX64.EFI then
  fs4:
  \EFI\DOOM\DOOMX64.EFI
  exit
endif
if exist fs5:\EFI\DOOM\DOOMX64.EFI then
  fs5:
  \EFI\DOOM\DOOMX64.EFI
  exit
endif
if exist fs6:\EFI\DOOM\DOOMX64.EFI then
  fs6:
  \EFI\DOOM\DOOMX64.EFI
  exit
endif
if exist fs7:\EFI\DOOM\DOOMX64.EFI then
  fs7:
  \EFI\DOOM\DOOMX64.EFI
  exit
endif
if exist fs8:\EFI\DOOM\DOOMX64.EFI then
  fs8:
  \EFI\DOOM\DOOMX64.EFI
  exit
endif
if exist fs9:\EFI\DOOM\DOOMX64.EFI then
  fs9:
  \EFI\DOOM\DOOMX64.EFI
  exit
endif

if exist fs0:\EFI\DOOM\BOOTX64.EFI then
  fs0:
  \EFI\DOOM\BOOTX64.EFI
  exit
endif
if exist fs1:\EFI\DOOM\BOOTX64.EFI then
  fs1:
  \EFI\DOOM\BOOTX64.EFI
  exit
endif
if exist fs2:\EFI\DOOM\BOOTX64.EFI then
  fs2:
  \EFI\DOOM\BOOTX64.EFI
  exit
endif
if exist fs3:\EFI\DOOM\BOOTX64.EFI then
  fs3:
  \EFI\DOOM\BOOTX64.EFI
  exit
endif
if exist fs4:\EFI\DOOM\BOOTX64.EFI then
  fs4:
  \EFI\DOOM\BOOTX64.EFI
  exit
endif
if exist fs5:\EFI\DOOM\BOOTX64.EFI then
  fs5:
  \EFI\DOOM\BOOTX64.EFI
  exit
endif
if exist fs6:\EFI\DOOM\BOOTX64.EFI then
  fs6:
  \EFI\DOOM\BOOTX64.EFI
  exit
endif
if exist fs7:\EFI\DOOM\BOOTX64.EFI then
  fs7:
  \EFI\DOOM\BOOTX64.EFI
  exit
endif
if exist fs8:\EFI\DOOM\BOOTX64.EFI then
  fs8:
  \EFI\DOOM\BOOTX64.EFI
  exit
endif
if exist fs9:\EFI\DOOM\BOOTX64.EFI then
  fs9:
  \EFI\DOOM\BOOTX64.EFI
  exit
endif

echo DOOMAUTO: no se encontro DOOMX64.EFI o BOOTX64.EFI en \EFI\DOOM\ de fs0..fs9
exit 1
