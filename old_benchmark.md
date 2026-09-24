AspectShift-HTOV — Sequential vs. Parallel Processing Benchmark

## 1. Benchmark Purpose

This benchmark compares batch video processing time between the **Sequential Processing** and **Parallel Processing** versions of AspectShift-HTOV.

Three benchmark turns were conducted for each processing version using the same processing configuration and input batches.

## 2. Test Conditions

### General Conditions

- No user applications or tasks were running in the background apart from normal system processes.
- The **Parallel Processing** version was run using `cargo tauri dev`.
- Three turns were performed for each processing version.
- Processing time was recorded for each batch run.

### Processing Configuration

| SettingValue   |                              |
| -------------- | ---------------------------- |
| Aspect Ratio   | 4:5 — built-in Reddit preset |
| Quality Preset | Standard                     |
| CRF            | 22                           |
| Speed Preset   | Medium                       |
| Audio Bitrate  | 128 kbps                     |
| Output Format  | MP4 (H.264)                  |
| Effect         | Background Blur — Sigma 20   |

### Input Batches

Two input batches were tested in every turn:

- **MP4 input:** `D:\other\crazy\batch input\mp4`
- **WebM input:** `D:\other\crazy\batch input\webm`

Outputs were stored in their corresponding directories:

- **MP4 batch:** `G:\crazy\batch output\mp4`
- **WebM batch:** `G:\crazy\batch output\webm`

---

# 3. Benchmark Results

## Turn 1

| Processing VersionInputProcessing Time |      |              |
| -------------------------------------- | ---- | ------------ |
| Sequential                             | MP4  | **03:07.10** |
| Sequential                             | WebM | **02:48.08** |
| Parallel                               | MP4  | **05:52.64** |
| Parallel                               | WebM | **05:16.94** |

## Turn 2

| Processing VersionInputProcessing Time |      |              |
| -------------------------------------- | ---- | ------------ |
| Sequential                             | MP4  | **03:44.23** |
| Sequential                             | WebM | **02:47.20** |
| Parallel                               | MP4  | **06:15.22** |
| Parallel                               | WebM | **05:24.85** |

## Turn 3

| Processing VersionInputProcessing Time |      |              |
| -------------------------------------- | ---- | ------------ |
| Sequential                             | MP4  | **04:02.90** |
| Sequential                             | WebM | **02:53.97** |
| Parallel                               | MP4  | **06:07.15** |
| Parallel                               | WebM | **05:20.33** |

---

# 4. Complete Benchmark Data

| TurnInputSequentialParallel |      |          |          |
| --------------------------- | ---- | -------- | -------- |
| 1                           | MP4  | 03:07.10 | 05:52.64 |
| 1                           | WebM | 02:48.08 | 05:16.94 |
| 2                           | MP4  | 03:44.23 | 06:15.22 |
| 2                           | WebM | 02:47.20 | 05:24.85 |
| 3                           | MP4  | 04:02.90 | 06:07.15 |
| 3                           | WebM | 02:53.97 | 05:20.33 |

---

# 5. Benchmark Scope

This benchmark records the observed processing times of the current **Sequential** and **Parallel** implementations under the conditions described above.

The results are intended to provide a baseline for investigating the performance of the parallel processing implementation and for comparing subsequent changes against the current behavior.
