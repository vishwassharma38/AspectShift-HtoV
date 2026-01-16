import os
import time
import subprocess
from watchdog.observers import Observer
from watchdog.events import FileSystemEventHandler

# =====================
# CONFIG
# =====================
INPUT_DIR = r"E:\Projects\Horizontal"
OUTPUT_DIR = r"E:\Projects\Vertical"
PROCESSING_EXT = ".processing"
SUPPORTED_EXTENSIONS = (".mp4", ".mkv", ".avi", ".mov", ".webm", ".flv", ".m4v")

WITH_LOGO = False            # <<< LOGO TOGGLE
LOGO_NAME = "logo.png"
LOGO_GAP = 32               # px gap below 4:5 frame

OUTPUT_WIDTH = 1080
OUTPUT_HEIGHT = 1920
FG_HEIGHT = 1350            # 4:5 height

os.makedirs(OUTPUT_DIR, exist_ok=True)

# =====================
# HELPER FUNCTIONS
# =====================
def wait_for_file_ready(filepath, timeout=30):
    last_size = -1
    start_time = time.time()

    while True:
        try:
            size = os.path.getsize(filepath)
            if size == last_size and size > 0:
                return True
            last_size = size
            time.sleep(1)

            if time.time() - start_time > timeout:
                print(f"⚠️ Timeout waiting for file: {filepath}")
                return False
        except OSError:
            time.sleep(1)

def get_logo_path(video_path):
    if not WITH_LOGO:
        return None

    logo_path = os.path.join(os.path.dirname(video_path), LOGO_NAME)
    return logo_path if os.path.isfile(logo_path) else None

# =====================
# VIDEO CONVERSION
# =====================
def convert_to_vertical(input_path):
    filename = os.path.basename(input_path)
    name, _ = os.path.splitext(filename)

    processing_flag = input_path + PROCESSING_EXT
    output_path = os.path.join(OUTPUT_DIR, f"{name}_vertical.mp4")

    if os.path.exists(processing_flag) or os.path.exists(output_path):
        return

    if not wait_for_file_ready(input_path):
        return

    try:
        open(processing_flag, "w").close()
    except PermissionError:
        print(f"⚠️ Could not lock file: {filename}")
        return

    logo_path = get_logo_path(input_path)

    try:
        print(f"🎬 Converting: {filename}")

        # -------------------------------
        # FILTER GRAPH
        # -------------------------------
        filter_chain = []

        # Background
        filter_chain.append(
            "[0:v]scale=1080:1920:force_original_aspect_ratio=increase,"
            "crop=1080:1920,gblur=sigma=20[bg]"
        )

        # Foreground 4:5
        filter_chain.append(
            "[0:v]scale=1080:1350:force_original_aspect_ratio=increase,"
            "crop=1080:1350[fg]"
        )

        # Place fg centered vertically
        fg_y = f"(H-{FG_HEIGHT})/2"
        filter_chain.append(
            f"[bg][fg]overlay=(W-w)/2:{fg_y}[v1]"
        )

        input_args = ["-i", input_path]

        # -------------------------------
        # LOGO LOGIC
        # -------------------------------
        if WITH_LOGO and logo_path:
            input_args += ["-i", logo_path]

            logo_y = f"({fg_y})+{FG_HEIGHT}+{LOGO_GAP}"

            filter_chain.append(
                "[1:v]scale="
                "min(iw\\,1080):"
                "min(ih\\,(1920-1350-32)):"
                "force_original_aspect_ratio=decrease[logo]"
            )

            filter_chain.append(
                f"[v1][logo]overlay=(W-w)/2:{logo_y}"
            )

            final_map = "[vout]"
            filter_chain[-1] += final_map
        else:
            final_map = "[v1]"

        cmd = [
            "ffmpeg",
            "-y",
            *input_args,
            "-filter_complex", ";".join(filter_chain),
            "-map", final_map,
            "-map", "0:a?",
            "-c:v", "libx264",
            "-preset", "slow",
            "-crf", "18",
            "-pix_fmt", "yuv420p",
            "-movflags", "+faststart",
            "-c:a", "aac",
            "-b:a", "192k",
            output_path
        ]

        subprocess.run(cmd, check=True)
        print(f"✅ Done: {output_path}")

    except subprocess.CalledProcessError:
        print(f"❌ FFmpeg error on: {filename}")
    except Exception as e:
        print(f"❌ Error: {e}")
    finally:
        if os.path.exists(processing_flag):
            os.remove(processing_flag)

# =====================
# WATCHDOG HANDLER
# =====================
class VideoHandler(FileSystemEventHandler):
    def on_created(self, event):
        if event.is_directory:
            return
        if event.src_path.lower().endswith(SUPPORTED_EXTENSIONS):
            print(f"👀 Detected: {os.path.basename(event.src_path)}")
            convert_to_vertical(event.src_path)

# =====================
# MAIN LOOP
# =====================
if __name__ == "__main__":
    print("=========================================")
    print("         AspectShift-HtoV"                )
    print("=========================================")
    print(f"📂 Watching: {INPUT_DIR}")
    print(f"💾 Output:   {OUTPUT_DIR}")
    print(f"🖼️  Logo:     {'ENABLED' if WITH_LOGO else 'DISABLED'}")
    print("-----------------------------------------")

    observer = Observer()
    observer.schedule(VideoHandler(), INPUT_DIR, recursive=False)
    observer.start()

    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        observer.stop()
        print("\n🛑 Stopped")

    observer.join()
