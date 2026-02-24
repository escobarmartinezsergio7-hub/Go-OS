# Creates basic directories on Windows PowerShell
$paths = @(
  "boot",
  "grub",
  "kernel\\src",
  "drivers",
  "libc",
  "include",
  "tools",
  "apps\\hello_redux",
  "sdk\\reduxlang\\src"
)

foreach ($p in $paths) {
  New-Item -ItemType Directory -Force -Path $p | Out-Null
}

Write-Host "ReduxOS starter directory tree created." -ForegroundColor Green
