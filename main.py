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
SUPPORTED_EXTENSIONS = (".mp4", ".mkv", ".avi", ".mov", ".webm", ".flv")

os.makedirs(OUTPUT_DIR, exist_ok=True)

# =====================
# VIDEO CONVERSION
# =====================
def convert_to_vertical(input_path):
    filename = os.path.basename(input_path)
    name, ext = os.path.splitext(filename)

    processing_flag = input_path + PROCESSING_EXT
    output_path = os.path.join(OUTPUT_DIR, f"{name}_vertical.mp4")

    # Skip if already processing or already converted
    if os.path.exists(processing_flag) or os.path.exists(output_path):
        return

    # Mark as processing
    open(processing_flag, "w").close()

    try:
        print(f"🎬 Converting: {filename}")

        cmd = [
            "ffmpeg",
            "-y",
            "-i", input_path,
            "-filter_complex",
            (
                "[0:v]scale=1080:1920:force_original_aspect_ratio=increase,"
                "crop=1080:1920,boxblur=20:1[bg];"
                "[0:v]scale=1080:-1:force_original_aspect_ratio=decrease[fg];"
                "[bg][fg]overlay=(W-w)/2:(H-h)/2"
            ),
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

        os.remove(processing_flag)

    except Exception as e:
        print(f"❌ Failed: {filename}")
        print("🔁 Will retry...")
        time.sleep(3)
        os.remove(processing_flag)
        convert_to_vertical(input_path)

# =====================
# WATCHDOG HANDLER
# =====================
class VideoHandler(FileSystemEventHandler):
    def on_created(self, event):
        if event.is_directory:
            return

        if event.src_path.lower().endswith(SUPPORTED_EXTENSIONS):
            time.sleep(2)  # wait for file copy to finish
            convert_to_vertical(event.src_path)

# =====================
# MAIN LOOP
# =====================
if __name__ == "__main__":
    print("👀 Watching for horizontal videos...")
    print(INPUT_DIR)

    event_handler = VideoHandler()
    observer = Observer()
    observer.schedule(event_handler, INPUT_DIR, recursive=False)
    observer.start()

    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        observer.stop()

    observer.join()
