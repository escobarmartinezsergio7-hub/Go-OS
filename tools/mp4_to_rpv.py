#!/usr/bin/env python3
import sys
import subprocess
import struct
import os

def convert_video(input_file, output_file, target_width=320, target_height=240, target_fps=60):
    if not os.path.exists(input_file):
        print(f"Error: {input_file} no encontrado.")
        sys.exit(1)
        
    print(f"Convirtiendo {input_file} a formato ZENOX OS RPV...")
    print(f"Resolución destino: {target_width}x{target_height} @ {target_fps} FPS")

    # Comando FFmpeg para extraer pixeles crudos BGRA
    cmd = [
        "ffmpeg",
        "-i", input_file,
        "-vf", f"scale={target_width}:{target_height}",
        "-r", str(target_fps),
        "-vcodec", "rawvideo",
        "-pix_fmt", "bgra",
        "-f", "image2pipe",
        "-"
    ]

    try:
        print("Ejecutando ffmpeg (esto puede tomar unos segundos)...")
        process = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        raw_video_data, _ = process.communicate()
        
        if process.returncode != 0:
            print("Error: Falló la ejecución de ffmpeg. ¿Está instalado ffmpeg en tu Mac?")
            sys.exit(1)
            
        print(f"Pixeles extraídos: {len(raw_video_data)} bytes.")

        # Armar la cabecera oficial RPV1 de ZENOX OS (16 bytes)
        # B"RPV1" (4 bytes)
        # width (4 bytes LE)
        # height (4 bytes LE)
        # fps (4 bytes LE)
        header = b"RPV1"
        header += struct.pack("<I", target_width)
        header += struct.pack("<I", target_height)
        header += struct.pack("<I", target_fps)

        with open(output_file, "wb") as f:
            f.write(header)
            f.write(raw_video_data)
            
        print(f"¡Éxito! Archivo {output_file} generado correctamente para ZENOX OS.")
    except Exception as e:
        print(f"Error fatal: {e}")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Uso: python3 mp4_to_rpv.py <entrada.mp4> <salida.rpv>")
        print("Perfil por defecto: 320x240 @ 60 FPS")
        print("Ejemplo: python3 tools/mp4_to_rpv.py pelicula.mp4 video_zenox.rpv")
        sys.exit(1)
        
    in_f = sys.argv[1]
    out_f = sys.argv[2]
    convert_video(in_f, out_f)
