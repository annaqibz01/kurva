# Product Requirement Document (PRD)

## Proyek: `kurva` (High-Performance Raster-to-Vector Engine)

* **Versi Dokumen**: 1.0.0-MVP
* **Status**: Ready for Implementation
* **Target Rilis**: v0.1.0 (MVP)
* **Arsitektur Stack**: Rust (Cargo Workspace Monorepo)

---

## 1. Ringkasan Eksekutif & Visi Produk

`kurva` adalah engine konversi *raster-to-vector* (vectorizer) open-source berbasis Rust yang dirancang untuk menghasilkan kurva vektor SVG presisi tinggi dengan sudut tajam (*sharp cusps*), segmentasi warna bertingkat (*cel-shading*), dan topologi bebas celah (*gapless*).

Produk ini memecahkan kelemahan utama engine open-source tradisional (seperti Potrace standar) yang kerap menghasilkan garis bergelombang (*jittery*), sudut tumpul pada grafis maskot, dan celah garis putih antar-warna, tanpa mengharuskan pengguna bergantung pada layanan komersial berbayar (Vector Magic / Vectorizer.AI).

---

## 2. Problem Statement & Value Proposition

### Permasalahan

* **Corner Blunting**: Algoritma trace umum menghaluskan sudut tajam sehingga paruh maskot, cakar, atau sudut teks geometris kehilangan ketajamannya.
* **Jittery Curves**: Piksel anti-aliased pada gambar raster dibaca sebagai noise tangga, memicu ledakan jumlah titik kontrol (*overfitting node*).
* **Boundary Gaps**: Penelusuran bidang warna yang saling independen memunculkan celah putih tipis (*white seams*) ketika diekspor ke SVG.
* **Monolithic Fragility**: Kebanyakan tracer open-source ditulis dalam satu kesatuan file besar yang sulit dimodifikasi oleh AI agent atau kontributor secara terisolasi.

### Value Proposition

* **Cusp-Preserving Fitting**: Membedakan titik sudut tajam (*cusp*) dengan sambungan mulus (*smooth joint*) secara matematis sebelum kurva di-fit.
* **Contract-Driven Modular Workspace**: Arsitektur modular *one-way pipeline* yang memungkinkan perbaikan pada modul matematika fitting tanpa menyentuh modul warna atau kontur.
* **Sub-100ms Deterministic Execution**: Memanfaatkan efisiensi zero-cost abstraction Rust untuk memproses grafis logo/maskot beresolusi tinggi dalam hitungan milidetik secara offline.

---

## 3. Target Pengguna & Kasus Penggunaan

### Target Pengguna

1. **Desainer Grafis & Apparel/Merchandise Specialist**: Membutuhkan vektorisasi bersih dari sketsa atau desain bitmap untuk sablon, sublimasi jersey, dan cutting plotter tanpa perbaikan manual yang lama.
2. **Pengembang Aplikasi Web & Sistem**: Membutuhkan library native Rust atau CLI tool ringan untuk otomatisasi pipeline konversi aset grafis di backend tanpa runtime Python/C++ yang berat.

### Kasus Penggunaan Utama

* Konversi logo esport/emblem (berkarakter maskot elang, perisai, tipografi balok) dari PNG/JPG menjadi SVG siap cetak.
* Vektorisasi line art monokrom dan ilustrasi bergaya *flat/cel-shaded*.

---

## 4. Batasan Ruang Lingkup (Scope Boundaries)

### Dalam Cakupan (MVP v0.1.0)

* Input berkas: PNG dan JPG (RGB/RGBA).
* Kuantisasi warna palet terbatas (1 hingga 8 warna).
* Ekstraksi kontur batas piksel (closed loop).
* Simplifikasi poligon RDP dengan klasifikasi sudut tajam vs mulus.
* Fitting kurva Bézier kubik presisi menggunakan varian Schneider's algorithm.
* Output SVG valid dengan penataan tumpukan layer (*layer stacking* dari gelap ke terang).
* Antarmuka Command Line Interface (CLI) one-shot.

### Di Luar Cakupan (Ditunda ke v0.2.0+)

* Ekstraksi continuous coverage sub-pixel floating point tingkat lanjut.
* Pengenalan primitif geometris otomatis (lingkaran eksak dan garis lurus tak terbatas).
* Struktur graf planar DCEL (Doubly Connected Edge List) dinamis penuh.
* Differentiable vector rasterization berbasis machine learning (DiffVG/PyTorch).
* Antarmuka GUI desktop atau plugin Figma/Inkscape.

---

## 5. Spesifikasi Kebutuhan Fungsional (Functional Requirements)

```
[Raw Bitmap] 
    ↓ (kurva-color)
[IndexedRaster] 
    ↓ (kurva-contour)
[RawContourList] 
    ↓ (kurva-poly)
[SimplifiedPolyline] 
    ↓ (kurva-fit)
[VectorPathList] 
    ↓ (kurva-svg)
[Clean SVG Output]

```

### Modul 1: `kurva-types` (Kontrak Data Sentral)

* **FR-1.1**: Wajib mendefinisikan tipe data geometris murni: `Point2D<T>` (mendukung `i32` dan `f64`).
* **FR-1.2**: Wajib menyediakan struktur representasi perantara (*Intermediate Representation*): `IndexedRaster`, `RawContour`, `SimplifiedPolyline`, `BezierSegment`, dan `VectorPath`.
* **FR-1.3**: Dilarang memiliki dependensi ke crate internal lain di dalam repositori `kurva`.

### Modul 2: `kurva-color` (Kuantisasi & Segmentasi Palet)

* **FR-2.1**: Menerima input byte raw RGB/RGBA dan resolusi gambar ($W \times H$).
* **FR-2.2**: Mengonversi piksel RGB ke ruang warna perceptually uniform (CIELAB) untuk meminimalkan distorsi jarak warna visual ($\Delta E$).
* **FR-2.3**: Mengelompokkan piksel menjadi $K$ palet warna representatif menggunakan K-Means Clustering terisolasi.
* **FR-2.4**: Mengembalikan `IndexedRaster` berisi palet warna unik dan buffer 1D/2D berisi `color_id` (`u8`).

### Modul 3: `kurva-contour` (Border Following)

* **FR-3.1**: Menerima `IndexedRaster`.
* **FR-3.2**: Mengekstrak rangkaian batas piksel tertutup (*closed loops*) untuk setiap `color_id` menggunakan algoritma Marching Squares atau border following Suzuki-Abe.
* **FR-3.3**: Mengidentifikasi relasi topologi sederhana: membedakan batas luar (*outer boundary*) dan lubang pulau di dalam bidang (*hole boundary*).
* **FR-3.4**: Mengeluarkan `RawContourList` berupa kumpulan titik koordinat diskrit integer (`Point2D<i32>`).

### Modul 4: `kurva-poly` (Simplifikasi & Deteksi Cusp)

* **FR-4.1**: Menerima `RawContourList`.
* **FR-4.2**: Mengeliminasi titik-titik tangga (*staircase artifacts*) menggunakan algoritma Ramer-Douglas-Peucker (RDP) dengan parameter ambang batas $\epsilon_{\text{poly}}$.
* **FR-4.3**: Menghitung sudut interior pada tiap simpul poligon menggunakan dot-product dua vektor tetangga:

$$\cos(\theta) = \frac{\vec{u} \cdot \vec{v}}{\Vert{}\vec{u}\Vert{} \Vert{}\vec{v}\Vert{}}$$

* **FR-4.4**: Mengklasifikasikan titik sebagai sudut tajam (*corner cusp*, `is_corner: true`) jika sudut $\theta < \theta_{\text{threshold}}$ (default: $120^\circ$). Simpul lainnya ditandai sebagai titik mulus (*smooth joint*).
* **FR-4.5**: Mengeluarkan `SimplifiedPolyline`.

### Modul 5: `kurva-fit` (Fitting Kurva Bézier Kubik)

* **FR-5.1**: Menerima `SimplifiedPolyline`.
* **FR-5.2**: Membagi poligon menjadi segmen-segmen terpisah pada setiap titik yang ditandai `is_corner: true`. Titik sudut tajam dilarang digabungkan ke dalam satu kurva halus melintasi batas sudut.
* **FR-5.3**: Mengimplementasikan algoritma Philip J. Schneider untuk mencari titik kontrol kurva kubik Bézier $P_1$ dan $P_2$ dengan minimisasi kuadrat terkecil (*least-squares fit*).
* **FR-5.4**: Jika jarak deviasi maksimum titik poligon terhadap kurva melebihi toleransi error $\epsilon_{\text{fit}}$, fungsi wajib membelah segmen di titik error terbesar dan melakukan fitting rekursif.
* **FR-5.5**: Menegakkan kontinuitas tangen $G^1$ hanya pada simpul yang berstatus *smooth joint*.
* **FR-5.6**: Mengeluarkan `VectorPathList`.

### Modul 6: `kurva-svg` (Serialisasi SVG)

* **FR-6.1**: Menerima `VectorPathList` dan metadata palet warna.
* **FR-6.2**: Menghasilkan elemen `<path>` valid standar W3C SVG menggunakan perintah `M` (*MoveTo*), `C` (*Cubic Bézier*), dan `Z` (*ClosePath*).
* **FR-6.3**: Memformat angka koordinat dengan pembulatan maksimal 2–3 digit desimal untuk menekan ukuran berkas SVG.
* **FR-6.4**: Menerapkan strategi layer stacking (menumpuk layer gelap di urutan awal dan layer highlight di atasnya dengan stroke offset tipis) guna mencegah celah visual antar-bidang warna.

### Modul 7: `kurva-cli` (CLI Interface)

* **FR-7.1**: Menyediakan antarmuka terminal dengan argumen mandatori dan opsional:
* `-i, --input <PATH>`: Path file raster (PNG/JPG).
* `-o, --output <PATH>`: Path output file SVG.
* `-c, --colors <INT>`: Jumlah palet warna (default: 4, range: 1–8).
* `--error-threshold <FLOAT>`: Nilai toleransi fitting kurva (default: 2.0).
* `--corner-threshold <FLOAT>`: Sudut batas deteksi cusp dalam derajat (default: 120.0).



---

## 6. Spesifikasi Kebutuhan Non-Fungsional (Non-Functional Requirements)

| Kategori | Parameter Kebutuhan |
| --- | --- |
| **Performa** | Memproses gambar maskot resolusi $2048 \times 2048$ piksel (4 warna) dalam waktu $\le 100\text{ ms}$ pada CPU modern standar (mode `--release`). |
| **Konsumsi Memori** | Alokasi memori puncak (*peak RAM usage*) tidak boleh melebihi $150\text{ MB}$ untuk gambar berukuran hingga 4K. |
| **Efisiensi Vektor** | Jumlah node (titik kontrol) SVG yang dihasilkan harus $50\%\text{ s/d }75\%$ lebih sedikit dibanding trace default Potrace tanpa kehilangan siluet bentuk. |
| **Portabilitas** | Kompilasi murni native Rust tanpa ketergantungan runtime eksternal (C-runtime dinamis, OpenCV, atau Python virtual environment). Berjalan langsung di Windows, Linux, dan macOS. |
| **Code Quality** | 100% bebas blok `unsafe` pada seluruh crate inti. Nol warning pada eksekusi `cargo clippy --all-targets -- -D warnings`. |
| **Batas Ukuran Berkas** | Seluruh file kode sumber `.rs` dibatasi maksimal **250 baris kode**. |

---

## 7. Rencana Struktur Repositori & Kontrak

```text
kurva/
├── Cargo.toml                  # Workspace definition
├── agent_rules.md              # Direktif teknis agen AI & kontributor
├── PRD.md                      # Dokumen spesifikasi kebutuhan produk
├── crates/
│   ├── kurva-types/            # Entitas domain & DTO perantara (Frozen)
│   ├── kurva-color/            # CIELAB & K-Means clustering
│   ├── kurva-contour/          # Marching Squares & border extraction
│   ├── kurva-poly/             # RDP simplifier & corner cusp detection
│   ├── kurva-fit/              # Schneider's curve fitting
│   ├── kurva-svg/              # XML/SVG emitter
│   └── kurva-cli/              # CLI interface (orchestrator)
└── fixtures/                   # Deterministic JSON mock data untuk testing
    ├── sample_indexed.json
    ├── sample_raw_contours.json
    └── sample_polylines.json

```

---

## 8. Verifikasi & Kriteria Penerimaan (Acceptance Criteria)

Sebuah implementasi dinyatakan selesai dan siap dimerge jika memenuhi seluruh kriteria berikut:

1. **Uji Validitas Geometri**: Seluruh lintasan Bézier kubik yang dihasilkan tertutup rapat (`Z`) dan tidak mengandung nilai `NaN` atau `Infinity` pada koordinatnya.
2. **Uji Preservasi Sudut**: Pada gambar uji siluet elang, sudut paruh dan cakar mempertahankan titik tajam (*sharp corner point*), terbukti dengan adanya diskontinuitas tangen kurva di simpul tersebut.
3. **Uji Isolasi Modul**: Setiap crate diuji menggunakan fixture JSON deterministik tanpa perlu menjalankan crate sebelumnya (`cargo test -p kurva-poly`, `cargo test -p kurva-fit`).
4. **Verifikasi Kualitas**: Lolos verifikasi build, zero clippy warning, dan zero failure test:
```powershell
cargo check
cargo clippy --all-targets -- -D warnings
cargo test

```



---

## 9. Rencana Rilis & Milestone

* **Milestone 1 (Fondasi & Tipe Data)**: Menyiapkan Cargo workspace, crate `kurva-types`, dan fixture data JSON.
* **Milestone 2 (Kuantisasi & Kontur)**: Implementasi `kurva-color` (CIELAB K-Means) dan `kurva-contour` (Marching Squares).
* **Milestone 3 (Geometri & Fitting)**: Implementasi `kurva-poly` (RDP + cusp detection) dan `kurva-fit` (Schneider algorithm).
* **Milestone 4 (Integrasi & Eksportir)**: Implementasi `kurva-svg` (layer stacking) dan `kurva-cli` (end-to-end binary).
* **Milestone 5 (Audit & Rilis v0.1.0)**: Benchmark latency, optimasi ambang batas default, dan rilis versi open-source publik di GitHub.

---
