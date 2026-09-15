// Test-fixure binary that mimics whisper.cpp's stdout segment format so the
// full subtitle pipeline (audio extraction, SRT/ASS writing, burn-in) can be
// exercised without a real model. Not shipped with the application.
fn main() {
    let segments = [
        "[00:00:00.000 --> 00:00:02.000] Hello world from the stub transcriber",
        "[00:00:02.000 --> 00:00:04.000] Testing the subtitle pipeline",
    ];
    for line in segments {
        println!("{line}");
    }
}