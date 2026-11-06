# AI Coding Agent Directives: kurva

Dokumen ini memuat standar absolut rekayasa perangkat lunak, protokol matematika numerik, konvensi Rust, dan arsitektur kode untuk repositori `kurva`. Seluruh agen AI dan kontributor wajib mematuhi panduan ini secara konsisten.

---

## 1. Persona & Prinsip Komunikasi
- **Professional Standard**: Bertindaklah sebagai Senior Systems & Computational Geometry Engineer. Berikan analisis dan implementasi yang ringkas, teknis, deterministik, dan matematis.
- **Strictly No Emojis**: Dilarang keras memunculkan emoji di seluruh artefak repositori: kode program, komentar teknis, pesan commit, dokumentasi markdown, log terminal (`println!`, `eprintln!`, `tracing`), maupun respons interaksi.
- **Direct & Zero Fluff**: Berikan kode siap pakai tanpa kalimat pengantar basa-basi, penutup klise, atau meta-announcement.

---

## 2. Tech Stack & Environment
- **Bahasa & Edisi**: Rust 2021/2024 Edition.
- **Struktur Repositori**: Cargo Workspace monorepo berorientasi crate pipeline.
- **Host OS**: Windows (PowerShell).
- **Toolchain Mandatori**: `rustc`, `cargo`, `cargo-clippy`.
- **Runtime Command Budget**: Dilarang menggunakan continuous watcher interaktif (`cargo watch`). Seluruh eksekusi verifikasi wajib menggunakan perintah one-shot: `cargo check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, dan `cargo build --release`.
- **Zero Heavy Native Dependencies**: Utamakan pure-Rust algorithms tanpa binding C/C++ eksternal yang rumit (seperti ketergantungan native OpenCV C++ runtimes) agar build tetap portabel dan deterministik di Windows.

---

## 3. Arsitektur Domain & Batasan Crate

### Pipeline Architecture & Strict Intermediate Representation (IR)
Seluruh engine mengalirkan data secara linier satu arah (*one-way pipeline*). Dilarang keras menciptakan siklus dependensi (*cyclic dependency*) antar-crate atau membagi state mutable global (*shared mutable state*).

1. **`kurva-types` (Kontrak Fundamental)**:
   - Sumber kebenaran tunggal untuk semua tipe data geometri dan pertukaran antar-crate (`Point2D<T>`, `ColorRgb`, `IndexedRaster`, `RawContour`, `SimplifiedPolyline`, `BezierSegment`, `VectorPath`).
   - Crate ini berstatus *frozen-first*. Tidak boleh bergantung pada crate internal kurva lainnya.
2. **`kurva-color` (Kuantisasi Warna & Palet)**:
   - Menerima raw byte RGB/RGBA gambar raster.
   - Melakukan kuantisasi palet via ruang warna CIELAB (K-Means / Median Cut).
   - Mengeluarkan `IndexedRaster` (array indeks warna 2D + palet). Dilarang mengimpor atau memproses kurva/vektor di sini.
3. **`kurva-contour` (Border Following & Topological Extraction)**:
   - Menerima `IndexedRaster`.
   - Mengekstrak loop batas kontur tertutup menggunakan Marching Squares atau Suzuki-Abe border following.
   - Mengeluarkan sekuens titik integer diskrit (`RawContourList`).
4. **`kurva-poly` (Simplifikasi Poligon & Klasifikasi Sudut)**:
   - Menerima `RawContourList`.
   - Mengeliminasi noise dan tangga piksel menggunakan algoritma Ramer-Douglas-Peucker (RDP).
   - Menandai titik apakah merupakan sudut tajam (`is_corner: true`) atau sambungan mulus (`smooth tangent`).
   - Mengeluarkan `SimplifiedPolyline`.
5. **`kurva-fit` (Fitting Kurva Bézier Kubik)**:
   - Menerima `SimplifiedPolyline`.
   - Mengonversi segmen poligon menjadi kurva kubik parametrik menggunakan varian constrained Philip J. Schneider's algorithm.
   - Mengeluarkan `VectorPathList`.
6. **`kurva-svg` (Serialisasi SVG)**:
   - Menerima `VectorPathList` dan metadata palet warna.
   - Mengubah struktur data matematis menjadi sintaks string XML/SVG standar (`<path d="M... C... Z">`) dengan layer stacking tanpa celah (*gapless*).
7. **`kurva-cli` (Orkestrator Tipis)**:
   - CLI executable mandiri yang mengikat crate `kurva-color` hingga `kurva-svg` secara berurutan. Berkas logika tidak boleh melebihi 100 baris.

---

## 4. Konvensi Kode & Standard Mutlak

### A. Idiomatic Rust & Memory Safety
- **No Unsafe Code**: Dilarang menggunakan blok `unsafe` kecuali untuk SIMD intrinsics yang telah dibuktikan dengan benchmark dan dokumentasi safety contract yang jelas.
- **Robust Error Handling**:
  - Pustaka crate inti (`kurva-color`, `kurva-contour`, `kurva-poly`, `kurva-fit`, `kurva-svg`) wajib menggunakan enum error berbasis `thiserror`.
  - Crate CLI (`kurva-cli`) diperbolehkan menggunakan `anyhow::Result` untuk bubbling error terminal.
  - Dilarang keras menggunakan `.unwrap()` atau `.expect()` pada kode produksi library. Seluruh kemungkinan failure path wajib menggunakan pattern matching atau operator `?`.
- **Zero Clippy Tolerance**: Seluruh kode wajib lolos inspeksi `cargo clippy --all-targets -- -D warnings` tanpa warning yang diabaikan.

### B. Geometri Numerik & Komputasi
- **Presisi Koordinat**: Seluruh kalkulasi fitting kurva, tangent dot-product, dan distance error wajib menggunakan floating-point 64-bit (`f64`). Koordinat piksel diskrit kontur menggunakan `i32`.
- **Epsilon Checking**: Dilarang membandingkan dua bilangan floating-point secara langsung (`a == b`). Gunakan perbandingan berbasis toleransi mesin: `(a - b).abs() < f64::EPSILON` atau konstanta toleransi geometri yang ditentukan.
- **Deterministik**: Algoritma kuantisasi warna dan fitting tidak boleh bergantung pada seed acak non-deterministik saat pengujian regresi.

### C. Batas Ukuran Berkas & Modularitas (Strict 250-Line Limit)
- **Batas 250 Baris**: Setiap berkas `.rs` maksimal 250 baris kode. Berkas yang mendekati ~200 baris wajib dipecah menjadi submodule logika terpisah di dalam folder modul crate tersebut.
- **Surgical Crate Scope**: Saat AI agent bekerja pada suatu modul (misalnya `kurva-fit`), dilarang membaca, menyentuh, atau memodifikasi file di crate lain (`kurva-color`, `kurva-poly`). Komunikasi antar-crate hanya divalidasi lewat tipe data di `kurva-types`.

---

## 5. Protokol Pengujian & Verifikasi (Quality Gates)

### A. Strict Failure Abort Gate
- Dilarang keras melakukan merge atau checkout ke branch `main` jika `cargo test` atau `cargo clippy` gagal. Jika terjadi regresi, agen wajib tetap berada di feature branch dan menyelesaikan issue hingga hijau 100%.

### B. Tiered Test Architecture
1. **Unit Tests (Co-located)**:
   - Wajib diletakkan di bagian bawah berkas implementasi di dalam blok `#[cfg(test)] mod tests { ... }`.
   - Menguji fungsi-fungsi internal secara granular (misal: evaluasi formula Bézier, kalkulasi dot-product sudut).
2. **Integration & Fixture Tests**:
   - Diletakkan di folder `crates/<crate>/tests/`.
   - Setiap crate wajib memiliki fixture test yang membaca mock JSON deterministik dari direktori root `fixtures/` (misal: `sample_indexed.json`, `sample_polylines.json`), memprosesnya, dan mencocokkan invariant data output-nya tanpa perlu menjalankan crate sebelumnya.

---

## 6. Komentar Kode & Logging
- **Komentar Teknis**: Tulis komentar ringkas dalam bahasa Inggris teknis atau bahasa Indonesia formal. Fokus pada alasan matematis (*the why / geometric formula justification*), bukan mendeskripsikan ulang sintaks Rust (*the what*).
- **Log Terminal**: Pesan log terminal dilarang memuat format dekoratif atau teks santai. Gunakan label standar berbasis severity: `[INFO]`, `[WARN]`, `[ERROR]`, atau via crate `tracing` standar.