# Kurumsal olgunluk incelemesi — Ağustos 2026

`docs/competitive-gaps-2026-08.md` **ürünün** ne yapamadığını ölçüyor. Bu
doküman **mühendisliğin** ne taşıyamadığını ölçüyor: aynı kod tabanı 21 commit
yerine 2100 commit, bir geliştirici yerine on geliştirici, ve "benim makinemde"
yerine "bir kurumun 300 makinesinde" olduğunda ilk kırılacak yerler.

## Yöntem ve doğrulama

Her bulgu bu repoya karşı **çalıştırılarak** doğrulandı — dosya okuyarak değil,
sayarak: `cargo tree -e features` ile bağımlılık özellikleri, AST'ye yakın bir
Python taramasıyla komut imzaları, `curl` ile release endpoint'i.

**Bu dokümanın ilk taslağındaki dört iddia o kontrolden geçemedi ve
düzeltildi** — repodaki `competitive-gaps` dokümanının kendi taslağına yaptığı
şeyin aynısı. Ne oldukları §15'te açıkça listelendi, çünkü bir denetim
raporunun kendi hata payını gizlemesi onu okunmaz yapar.

**Ölçüm kapsamı.** 35.603 satır Rust (49 modül), 22.355 satır Vue/JS, 478 Rust
testi, 13 vitest dosyası, **143** IPC komutu, 17 MCP aracı, 2 dil.

---

## 0. Özet — olgunluk karnesi

| Alan                           |   Puan   | Tek cümlelik gerekçe                                                |
| ------------------------------ | :------: | ------------------------------------------------------------------- |
| Kod kalitesi & gerekçelendirme | **9/10** | Yorumlar karar kaydı seviyesinde; sektörde nadir.                   |
| Hata yönetimi (Rust)           | **9/10** | Üretim kodunda **7** `unwrap/expect` — hepsi bilinçli invariant.    |
| Sözleşme bütünlüğü (kod)       | **9/10** | Ölçüldü: kayıt ↔ implementasyon ↔ sözleşme **sıfır drift**.         |
| Güvenlik modeli (tasarım)      | **8/10** | argv-only spawn, yol sınırlama, dar capability, allowlist'li URL.   |
| Tedarik zinciri                | **7/10** | `cargo-deny` + Dependabot + `npm audit`; SBOM ve provenance yok.    |
| Test **stratejisi**            | **5/10** | 478 test var ama kapsam ölçülmüyor, E2E yok, sıcak modüller zayıf.  |
| Gözlemlenebilirlik             | **5/10** | Mükemmel log altyapısı, ama panic hook yok ve `panic = "abort"`.    |
| Mimari katmanlama              | **4/10** | 6.195 satırlık tek `commands.rs`; 48 komut `AppHandle`'a yapışık.   |
| Sürüm mühendisliği             | **3/10** | `pubkey: ""` **ve** güncelleme endpoint'i 404. Dağıtılamaz.         |
| Tip güvenliği (uçtan uca)      | **3/10** | IPC sınırında tip yok; 22k satır JS Rust'ın struct'larını bilmiyor. |
| Dokümantasyon doğruluğu        | **3/10** | README'de iki ölçülebilir iddia yanlış; SECURITY.md linki 404.      |
| i18n mimarisi                  | **5/10** | Kod tarafı doğru tasarlanmış; 146 spesifik dize çevrilmiyor.        |
| Erişilebilirlik                | **3/10** | Tek regex testi; axe yok, klavye/focus testi yok, RTL yok.          |
| Performans mühendisliği        | **2/10** | Tek benchmark yok, bundle bütçesi yok, cache stratejisi yok.        |
| Yönetişim & süreklilik         | **2/10** | Bus factor 1; ADR yok, ARCHITECTURE.md yok.                         |
| Kurumsal dağıtım               | **1/10** | Merkezî politika, private registry, air-gap — hiçbiri yok.          |

**Teşhis.** Bu, _tek bir çok iyi mühendisin_ yazabileceği en iyi kod
tabanlarından biri — ve tam olarak o yüzden kurumsal değil. Eksikler kod
kalitesinde değil; **kalitenin kod dışına, otomatik ve devredilebilir hale
çıkarılmasında.** Kod içinde doğrulama var (E/F suite'leri, differential
testler); kodun _çevresinde_ — README, release, dağıtım, devir — yok.

---

## 1. Önce doğru olan: neyi bozmamak gerekiyor

Aşağıdaki eleştirilerin hiçbiri bunları geçersiz kılmıyor:

1. **Gerekçe kaydı olarak yorumlar.** `elevate.rs`'in `mkcert -install`
   anlatısı, `atomic.rs`'in varlık nedeni, `quickcmd.rs`'in "katalog güvenlik
   modelidir" açıklaması. Çoğu şirketin Confluence'ında bu kalitede yazı yok.
2. **Differential doğrulama.** Bash generator'ını "muhtemelen aynı" diye değil,
   bayt bayt fixture karşılaştırmasıyla değiştirmek
   (`tests/fixtures_differential.rs`).
3. **Üretim kodunda `unwrap` yok.** 49 modülde toplam **7** tane
   _(ölçüm: her dosyada `#[cfg(test)]` öncesi bölüm)_, üçü `contracts.rs`'te
   derlenmiş JSON'un parse'ı. Çoğu Rust projesinin geçemediği bir çıta.
4. **Sözleşme bütünlüğü — ölçüldü, sıfır drift.**
   `lib.rs` **143** komut kaydediyor, `commands.rs` **143** komut uyguluyor,
   fark **yok**. `contracts/ipc.json` 147 komut bildiriyor: 143 Rust komutu +
   3 `kind: "frontend-plugin"` (bilinçli olarak frontend'de) + 1
   `status: "deferred"` (`updates_check`). Sözleşme kendi istisnalarını
   _makine-okunur alanlarla_ işaretliyor. Bu, bu incelemede rastlanan en
   olgun tek pratik.
5. **Kilitleme tasarımı.** `inflight::Registry` (kullanıcı hatası → anında
   reddet) ile `generate_lock` (dahili adım → sıraya al) ayrımı.
6. **`git::parse` allowlist'i.** `ext::sh`, `--upload-pack`, `file://`
   üçlüsünün ayrı ayrı gerekçelendirilip reddedilmesi.
7. **Hata sunumu tasarımı.** `ErrorAlert.vue` çevrilmiş kategori başlığını
   spesifik mesajın üstünde gösteriyor — ikisinden birini feda etmek yerine.
   Doğru tasarım; eksik olan tarafı §7'de.

---

## 2. Mimari — en pahalı borç

### 2.1 `commands.rs` bir tanrı modülü

**Ölçüm.** 6.195 satır, **143 `#[tauri::command]`**, tek dosya. `lib.rs`'teki
kayıt listesi elle bakımı yapılan 143 satır.

**Bugün drift yok** (§1.4) — bu bir _risk_ bulgusudur, bir kusur değil. Ama
riski taşıyan mekanizma zayıf: bir komutu `commands.rs`'e yazıp `lib.rs`'e
eklememek **derlenir ve sessizce geçer**. Yakalayan tek şey
`tools/validate-contracts.mjs` suite E, ve o CI job'ı **harici bir repo
checkout'una** bağımlı (`stackvo/stackvo`; bugün erişilebilir — doğrulandı,
HTTP 200). O repo private olur, silinir ya da rate-limit'e takılırsa sözleşme
kapısı kaybolur ve kimse fark etmez.

**Asıl fatura: 48 komut `AppHandle`'a yapışık.**

**Ölçüm.** 143 komutun **48'i** imzasında `AppHandle` taşıyor. Sözleşmeye göre
49 mutasyondan **20'si** öyle.

Bunun sonucu somut: iş mantığı Tauri'nin olay sistemine bağlı olduğu için,
Tauri olmayan her tüketici o mantığa erişemiyor. MCP sunucusu **17 araç**
sunuyor — 143 komutluk yüzeyin %12'si. Rekabet raporunun istediği "yardımcı
CLI" de aynı duvara çarpacak.

**"Yerine bu olsaydı."** Cargo workspace, üç crate:

```
crates/
  stackvo-core/     # Tauri'ye sıfır bağımlılık. Domain + IO.
                    #   trait ProgressSink { fn line(&self, …); fn finish(&self, …); }
                    #   pub async fn project_build(ws: &Workspace, sink: &dyn ProgressSink, …)
  stackvo-tauri/    # commands/ dizini, modül başına. Her komut 5–15 satır:
                    #   deserialize → core çağır → EventSink ile sarmala.
  stackvo-mcp/      # Aynı core, NullSink ya da JSON-RPC notification sink'i ile.
```

`ProgressSink` trait'i tek başına 48 komutun bağımlılığını bir implementasyona
indirir: MCP kendi sink'ini verir, testler `Vec<String>` toplayan bir sink
verir, gelecekteki CLI stdout sink'i verir. **Projedeki en yüksek getirili tek
değişiklik.**

### 2.2 IPC yüzeyi üretilmiyor, elle yazılıyor

Aynı 143 komut **dört yerde** ayrı ayrı yazılı: `commands.rs`, `lib.rs`,
`src/lib/ipc.js` (367 satır), `contracts/ipc.json`. Dört kaynak, bir gerçek —
ve tutarlılığı koruyan şey derleyici değil, bir Node betiği.

**"Yerine bu olsaydı."** `tauri-specta` (veya `ts-rs`): Rust komutlarından
TypeScript tipleri **ve** çağrı sarmalayıcıları üretilir.

- `ipc.js`'in tamamı ve suite E **gereksizleşir** — üretilen kod drift edemez.
- `projectsList()` dönüşü `Project[]` olur, `any` değil. Bugün Rust'ta bir alan
  adı değişse frontend sessizce `undefined` gösterir.
- `contracts/ipc.json` bildirim olmaktan çıkıp üretimin **girdisi** olur.

### 2.3 Frontend'de tanrı bileşenler

**Ölçüm.** `src/views/Settings.vue` **3.366 satır** = 1.113 satır `<script
setup>` + 2.057 satır `<template>`. İçinde **80 reaktif tanım** (50 `ref` + 30
`computed`), **27 farklı** `api.` çağrısı, 36 fonksiyon.
`src/views/ProjectDetail.vue` **3.007 satır**.

**Test durumu — nüanslı.** İki test (`template-overrides.spec.js`,
`certificates-pane.spec.js`) Settings.vue'yu **mount etmiyor**; panelin bir
_kopyasını_ test içinde yeniden kuruyor, sonra gerçek dosyayı metin olarak
okuyup kopyanın hâlâ eşleştiğini doğruluyor ("shape mirror" tekniği).

Bu, tanrı bileşeni test etmenin _yaratıcı_ bir çözümü ve yorumları neden böyle
yapıldığını iyi anlatıyor. Ama karşılığı şu: davranış **kopyada** doğrulanıyor,
**üründe** değil. Kopya ile gerçek arasındaki bağ bir `toContain(...)` string
eşleşmesi — bir boşluk değişikliği testi kırar, gerçek bir regresyon ise
kopyaya yansımadığı sürece kaçar.

**"Yerine bu olsaydı."** Settings zaten sekmeli — her sekme kendi bileşeni,
kendi composable'ı (`useCertificates()`, `useEnvEditor()`, `useTemplates()`) ve
kendi **mount edilen** testi olmalıydı. `SettingsGroup.vue`/`SettingsSection.vue`
doğru fikrin başlangıcı ama yalnızca sunumda kaldı; durum tek dosyada kaldı.
Pratik kural: **bir `.vue` dosyası 400 satırı geçtiğinde bölünür.**

---

## 3. Test stratejisi — sayı iyi, strateji yok

### 3.1 Kapsam ölçülmüyor

**Ölçüm.** `package.json`'da `--coverage` yok, `vitest.config.js`'te `coverage`
bloğu yok, CI'da `cargo-llvm-cov`/`tarpaulin` yok.

478 test etkileyici — ama neyin test edilmediği bilinmiyor. Modül başına
yoğunluk bunu ima ediyor:

| Modül         | Satır | Test | Not                                        |
| ------------- | ----: | ---: | ------------------------------------------ |
| `engine.rs`   | 1.391 |    4 | Docker'a dokunan her şeyin merkezi         |
| `pty.rs`      |   501 |    4 | Kullanıcı makinesinde süreç açıyor         |
| `scaffold.rs` |   791 |    5 | 28 şablon, hepsi kullanıcı dosyası yazıyor |
| `watcher.rs`  |   193 |    4 | Dosya sistemi olayları                     |
| `error.rs`    |   134 |    0 | Serileştirme şekli sözleşmenin parçası     |

**"Yerine bu olsaydı."** CI'da `cargo llvm-cov` + `vitest --coverage`. Rakam
önemli değil, **eşiğin var olması** önemli: eşiksiz kapsam, düşünce olmadan
yazılan teste ödül verir.

### 3.2 E2E yok

**Ölçüm.** `tauri-driver`, WebDriver, Playwright — hiçbiri yok.

`npm run diagnose` gerçekten değerli bir headless kontrol ama **arayüze hiç
dokunmuyor**. "Uygulama açılıyor ve bir proje başlatılabiliyor" iddiasını
doğrulayan otomatik hiçbir şey yok.

**"Yerine bu olsaydı."** `tauri-driver` + WebdriverIO ile beş smoke senaryosu:
açılış → workspace seçimi → proje oluştur → başlat → logları gör. Linux
runner'da `xvfb` ile çalışır; Tauri'nin resmî yolu budur.

### 3.3 Docker'sız test edilemeyen kod

`engine.rs`, `db.rs`, `phpini.rs`, `migrate.rs` gerçek bir daemon olmadan test
edilemiyor. Bu yüzden edilmiyorlar.

**"Yerine bu olsaydı."** Bollard çağrılarını `trait DockerEngine` arkasına
almak; testler sahte implementasyon verir, isteğe bağlı bir CI job'ı aynı
testleri gerçek daemon'a karşı koşar — Bash generator'ı için zaten kullanılan
differential mantığın aynısı.

### 3.4 Property-based test yok

`generator.rs` (1.982 satır) metin üretiyor ve fixture'larla karşılaştırılıyor.
Fixture'lar **bilinen** girdileri kapsar. `proptest` ile "hangi geçerli manifest
verilirse verilsin çıktı geçerli YAML'dır ve proje adı kaçışlanmıştır"
invariant'ı, elle yazılmış hiçbir fixture'ın bulamayacağı sınır durumlarını
bulur. Aynısı `config.rs` parser'ı ve `paths.rs` dönüşümleri için de geçerli.

---

## 4. Gözlemlenebilirlik — iyi altyapı, kritik bir delik

### 4.1 Panic sessiz ölüm — **en yüksek öncelikli düzeltme**

**Ölçüm.** `Cargo.toml` → `[profile.release] panic = "abort"`. Kod tabanında
`std::panic::set_hook` **yok** (`inflight.rs:142`'deki `catch_unwind` bir test).

Sonuç: release build'de herhangi bir panic — bir slice index, bir bağımlılıktaki
bug — uygulamayı **hiçbir iz bırakmadan** öldürür. Kullanıcı "kapanıp gitti"
der; `applog.rs`'in yazdığı rotasyonlu logda son satır normal bir bilgi
satırıdır.

**"Yerine bu olsaydı."** `logging::init()`'in hemen yanında:

```rust
std::panic::set_hook(Box::new(|info| {
    tracing::error!(panic = %info, backtrace = ?std::backtrace::Backtrace::force_capture());
    // Ayrıca ayrı bir crash-<tarih>.txt: log rotasyonu onu düşürmesin.
}));
```

~15 satır. Projenin diğer her yerindeki özenle en tutarsız eksik bu.

### 4.2 Tanılama paketi yok

Settings bugün log **klasörünü açıyor**; kullanıcıdan doğru dosyayı bulup
okuyup issue'ya eklemesi bekleniyor.

**"Yerine bu olsaydı."** Tek düğme: maskeli log + `preflight` + `doctor` +
`engine_status` + sürüm/platform bilgisi → tek zip. Maskeleme altyapısı
`applog.rs`'te zaten var; eksik olan yalnızca paketleyici.

### 4.3 Hiçbir kullanım verisi yok

Hangi preflight adımı en çok başarısız oluyor, hangi scaffold şablonu hiç
seçilmiyor — bilinmiyor. Bu bir gizlilik erdemi olarak savunulabilir ve
savunulmalı, ama **bilinçli bir karar olarak yazılı olmalı**; şu an sadece yok.

**"Yerine bu olsaydı."** Varsayılan kapalı, ne gönderdiği tek ekranda listelenen
opt-in telemetri — ya da SECURITY.md'de "telemetri yoktur ve olmayacaktır"
satırı. İkisi de kabul edilebilir; belirsizlik değil.

---

## 5. Güvenlik — tasarım güçlü, çevre eksik

### 5.1 `elevate::shell` string interpolasyonu

**Ölçüm.** `elevate.rs:48` —
`format!(r#"do shell script "{command}" with administrator privileges"#)`.

Modülün kendi yorumu bunu kabul ediyor: _"her çağıran ne geçirdiğinden
sorumlu."_ İki çağıran da uygulama yollarından kuruyor — ama o yollar kullanıcı
home dizinini ve `STACKVO_ROOT`'u içeriyor. Tek savunma bir yorum ve şeklin
pinlendiği bir test.

**"Yerine bu olsaydı."** Artan maliyetle üç seçenek:

1. **Asgari:** AppleScript quoting'i bir fonksiyona alıp (`"` → `\"`, `\` →
   `\\`) tırnaklı yol içeren bir testle sabitlemek. Bir saat.
2. **Doğrusu:** Script'i `on run argv` ile yazıp yolu `argv` üzerinden vermek.
   Enterpolasyon tamamen ortadan kalkar.
3. **Kurumsal:** macOS'ta `SMAppService` privileged helper, Linux'ta polkit
   policy dosyası — kurumsal dağıtımda zaten gereken şey.

### 5.2 Sırlar düz metinde

`.env` içinde veritabanı şifreleri düz metin. `env_reveal` bunu bilinçli ve
kontrollü açıyor — iyi. OS keystore (Keychain / Credential Manager / libsecret)
entegrasyonu yok.

Kurumsal karşılığı net: bir şirket makinesinde `~/.stackvo/.env` yedeklenen,
senkronlanan ve DLP taramasına takılan bir dosyadır. **"Yerine bu olsaydı":**
`SERVICE_*_PASSWORD` sınıfı anahtarlar keystore'da, `.env`'de
`keychain:stackvo/mysql-root` gibi bir referans. Bash CLI uyumluluğu bunu
zorlaştırır — bu bir _v2 sözleşme değişikliği_, ama şimdi planlanmalıydı.

### 5.3 Tedarik zinciri: SBOM ve provenance yok

`cargo-deny` + Dependabot + `npm audit` iyi bir taban. Eksik olan üçü de
kurumsal satın almada **sorulan** şeyler:

- **SBOM** (CycloneDX/SPDX) — `cargo cyclonedx` + `npm sbom`, CI'da beş satır.
- **Build provenance** (SLSA) — `actions/attest-build-provenance`, üç satır.
- **Artefakt checksum'ları** — `latest.json` imzalı, ama manuel indiren için
  SHA-256 listesi yok.

### 5.4 macOS sistem proxy'si okunmuyor _(düzeltilmiş bulgu — §15)_

**Ölçüm** (`cargo tree -e features -i reqwest`):

- ✅ **Sistem trust store KULLANILIYOR.** `reqwest`'in `rustls` özelliği
  `rustls-platform-verifier 0.7.0` çekiyor — macOS'ta `security-framework`,
  Windows'ta `windows-sys`, Linux'ta `rustls-native-certs`. **Kurumsal
  MITM-inspeksiyon CA'sı çalışır.** (`webpki-root-certs` graf içinde ama
  yalnızca `rustls-platform-verifier-android` altında.)
- ⚠️ **`macos-system-configuration` özelliği açık DEĞİL.** `default-features =
false` bunu kapatıyor ve `tauri-plugin-updater` da açmıyor (yalnızca
  `rustls-tls`, `json`, `stream`, `zip`).

Pratik sonuç, ilk taslakta iddia edilenden **çok daha dar**: `HTTPS_PROXY` /
`HTTP_PROXY` ortam değişkenleri her platformda okunur, ama **macOS Sistem
Ayarları'ndaki proxy** okunmaz. Yalnızca sistem ayarlarından proxy tanımlı bir
macOS makinesinde güncelleme kontrolü sessizce başarısız olur.

**"Yerine bu olsaydı."** `reqwest`'e `macos-system-configuration` özelliğini
eklemek (tek satır) — ve daha önemlisi, **güncelleme hatasının görünür
olması**: bugün `updater_status` başarısızlığı kullanıcıya nasıl gösteriliyor,
test edilmiş bir yol değil.

---

## 6. Sürüm mühendisliği — bugün fiilen dağıtılamaz

### 6.1 İki bağımsız blokaj, ikisi de doğrulandı

1. **İmza anahtarı yok.** `tauri.conf.json` → `plugins.updater.pubkey: ""`.
   `release.yml` preflight bunu doğru şekilde bloke ediyor.
2. **Güncelleme endpoint'i 404.**
   `https://raw.githubusercontent.com/stackvo/stackvo-tauri/main/latest.json`
   → **HTTP 404**. `stackvo/stackvo-tauri` reposu erişilebilir değil.
   (Karşılaştırma: `stackvo/stackvo` → HTTP 200, yani `contracts` CI job'ı
   bugün çalışıyor.)

İkisi birlikte: bu pipeline **hiç uçtan uca çalıştırılmamış**. README bunu
"sizin tedarik etmeniz gereken iki şey" diye anlatıyor; bir kurumsal okuyucu
için bu satırın anlamı budur.

**Yan etki — SECURITY.md'deki bildirim linki de ölü.** Aynı repoyu işaret
ediyor: `https://github.com/stackvo/stackvo-tauri/security/advisories/new`.
Bir güvenlik açığı bildirmek isteyen kişinin tıklayacağı bağlantı 404 veriyor;
geriye yalnızca e-posta kalıyor.

### 6.2 Sürüm numarası üç yerde

`package.json` `0.1.0`, `Cargo.toml` `0.1.0`, `tauri.conf.json` `0.1.0` —
**bugün uyumlu** (doğrulandı), ama uyumu koruyan hiçbir kontrol yok.
**"Yerine bu olsaydı":** üç değerin eşitliğini kontrol eden altı satırlık bir
test.

### 6.3 Kanal, kademeli dağıtım, geri alma yok

Tek `latest.json`, tek kanal. Kötü bir sürüm çıktığında yapılabilecek tek şey
yeni sürüm çıkarmaktır — o da güncelleme almış herkese anında gider.

**"Yerine bu olsaydı."** `stable`/`beta` kanalları, `latest.json`'da yüzdelik
kademeli dağıtım alanı, "bu sürümü durdur" anahtarı. Tauri updater'ı endpoint
şablonu destekliyor; maliyet düşük.

### 6.4 Platform kapsamı ve imzalama asimetrisi

`release.yml` dört hedef üretiyor. Eksikler: **Linux aarch64**, **Windows
ARM64**. Linux'ta Flatpak/AUR/Snap yok, `.deb` GPG imzası yok.

**Asimetri:** Windows sertifikası yoksa `::warning::` basılıyor; **macOS
notarizasyon secret'ı yoksa hiçbir uyarı yok** — imzasız bundle sessizce
yayınlanıyor ve Gatekeeper uyarısı kullanıcıya çıkıyor. Bir gözden kaçma.

---

## 7. i18n — tasarım doğru, kapsama eksik _(düzeltilmiş bulgu — §15)_

### 7.1 Kategori çevriliyor, spesifik metin çevrilmiyor

**Ölçüm.** `en.js`/`tr.js` içinde `errors` bloğu **13 anahtar** taşıyor —
`Code` enum'undaki 12 kodun **tamamı** artı `UNKNOWN`. `ErrorAlert.vue:30`
bunu başlık olarak gösteriyor, spesifik Rust mesajını altında bırakıyor. Bu
**bilinçli ve doğru** bir tasarım; bileşenin kendi yorumu nedenini anlatıyor.

Gerçek boşluk daha dar ve hâlâ gerçek. Ölçüm, 49 modülün `#[cfg(test)]` öncesi
bölümleri üzerinde:

- **113** `Error::new(Code::…)` spesifik mesajı İngilizce sabit — 21'i düz
  dize, 87'si `format!`, 5'i başka bir ifade.
- **33** `with_hint("…")` önerisi İngilizce sabit — ve `ErrorAlert.vue:92`
  bunu **ham olarak** basıyor.

Toplam **146** kullanıcıya görünebilen, çevrilmeyen dize.

Yani Türkçe arayüzde kullanıcı çevrilmiş bir başlık, altında İngilizce bir
açıklama ve İngilizce bir öneri görüyor. Öneri, kullanıcının **eyleme
geçeceği** metindir — çevrilmemesi en pahalı olanı odur.

**"Yerine bu olsaydı."** Altyapı hazır. `hint` için de kod tabanlı anahtar:
`Error` bir `hint_key` taşısın, `details` interpolasyon değerlerini versin,
frontend `errors.${code}.hints.${hint_key}` ile çevirsin. `message` yalnızca
log ve fallback olarak kalsın. Yan fayda: bugün 49 dosyaya dağılmış hata
metinleri tek locale dosyasında toplanır ve gözden geçirilebilir olur.

### 7.2 Dil sayısı koda gömülü

**Ölçüm.** `lib.rs` → `let turkish = … == "tr"; let labels = if turkish { … }
else { … }`. Tray ve menü etiketleri Rust'ta sabit.

Üçüncü dil eklendiğinde bu blok değişmek zorunda. Rakiplerde 14–30 dil var
(bkz. rekabet raporu); mevcut yapıyla üçüncü dil bile bir kod değişikliği.

**"Yerine bu olsaydı."** Menü/tray etiketlerini frontend'in `tray_relabel` ile
beslemesi — o komut zaten kayıtlı ve i18n frontend'de zaten çalışıyor.

### 7.3 RTL yok

**Ölçüm.** `vuetify.js` ve `i18n/index.js`'te `rtl` yapılandırması yok. Arapça/
Farsça/İbranice desteği bir bayraktan ibaret değil ama onunla başlar.

---

## 8. Erişilebilirlik

**Ölçüm.** `tests/a11y.spec.js` — tek test, regex ile ikon düğmelerinde
erişilebilir isim arıyor.

O test **doğru düşünülmüş** (tooltip'in neden yetmediğini açıklıyor, ilk
ölçümün neden yanlış olduğunu kaydediyor). Ama a11y'nin küçük bir dilimi.

Test edilmeyen: klavye ile tam gezinilebilirlik, focus tuzakları (bu uygulama
drawer/sheet/dialog yoğun), focus görünürlüğü, renk kontrastı (özellikle
`appearance.js`'in sistem vurgu renginden türettiği temada), canlı bölge
duyuruları (operasyon konsolu akan metin — ekran okuyucuya ne oluyor?), form
hata ilişkilendirmesi.

**"Yerine bu olsaydı."** `vitest-axe` ile mount edilen her bileşene otomatik axe
taraması — mevcut mount testlerine üç satır ekleme. Artı kritik akışlar için
klavye-only bir E2E senaryosu.

Kurumsal boyut: kamu sektörü ve büyük şirket satın almalarında **VPAT / EN 301
549 beyanı** istenir. Bugün üretilemez.

---

## 9. Performans

**Ölçüm.** `Cargo.toml`'da `criterion` yok, `benches/` dizini yok, bundle boyut
bütçesi yok.

CHANGELOG "5.2 MB → 2.1 MB" diyor — yani boyut bir kez elle ölçüldü ve bir daha
ölçülmedi. Bir sonraki font/ikon eklemesi sessizce geri alır.

Ölçülmemiş sıcak yollar:

- **`list_projects`** (`commands.rs:200`) — her çağrıda `read_dir` + her proje
  için `stackvo.json` okuması + bir Docker sorgusu. **Cache yok.** 50 projeli
  bir workspace'te davranışı bilinmiyor.
- **`generator.rs`** render süresi — her `up`/`build` bunu çalıştırıyor.
- **Arka plan döngüleri:** engine 5 sn, tray 15 sn, stats 60 sn. Uygulama
  tray'deyken bile aynı hızda dönüyor; dizüstünde pil maliyeti ölçülmemiş.

**"Yerine bu olsaydı."** (a) `criterion` ile generator ve manifest parse için
iki benchmark, CI'da regresyon eşiğiyle. (b) Bundle için `size-limit` ve CI
kapısı. (c) Pencere gizliyken poll aralığını uzatan tek bir
`if window.is_visible()` kontrolü.

---

## 10. Durum ve kalıcılık

- **Bozuk tercih dosyası sessizce siliniyor.** `commands.rs:4805` —
  `serde_json::from_str(&text).unwrap_or_else(|_| default_prefs())`. Çökmemesi
  doğru; ama kullanıcının **tüm ayarları** hiçbir uyarı olmadan varsayılana
  döner ve bozuk dosya yedeklenmez. `schemaVersion` alanı da yok, dolayısıyla
  ileride şema değiştiğinde migration yapacak bir tutamak yok.
  **"Yerine bu olsaydı":** `{ "schemaVersion": 1, … }`, parse hatasında
  `prefs.corrupt-<tarih>.json` olarak yedekle + kullanıcıya bir kez bildir.
- **`stats_history` bellekte.** `AppState` içinde `Mutex<StatsHistory>`; süreç
  ömrü kadar yaşıyor. Yorum web UI'ın "restart'ta ölüyordu" sorununu çözdüğünü
  ima ediyor, ama bu sürüm de uygulama yeniden başlayınca sıfırlanıyor. SQLite
  veya basit bir JSONL gerçek fark yaratır.
- **Mutex poisoning kalıcı bozukluk.** 8 çağrı yerinde `lock()` hatası
  `IoError`'a çevriliyor (`commands.rs` 3, `pty.rs` 4, `inflight.rs` 1). Bir
  thread panic'lediğinde o mutex sonsuza kadar zehirli kalır ve o özellik
  uygulama yeniden başlatılana kadar ölür. `prefs_set`'in
  `unwrap_or_else(|e| e.into_inner())` kullanması doğru desen — diğerleri
  değil. `parking_lot` (poisoning yok) ya da bilinçli kurtarma.

---

## 11. Dokümantasyon doğruluğu — projenin kendi tezine aykırı tek yüzey

Bu projenin tezi şu: _"'muhtemelen aynı' shipping için bir standart değil."_
Kod bu teze uyuyor (E/F suite'leri, differential testler, `mcp.rs`'te tool ↔
komut çapraz kontrolü). **README bu tezin dışında kalmış tek yüzey** — ve
ölçülebilir iki iddiası bugün yanlış:

| README iddiası                                                               | Ölçülen                            | Fark |
| ---------------------------------------------------------------------------- | ---------------------------------- | ---- |
| _"Thirty-four commands take an `AppHandle`"_ (satır 152)                     | **48**                             | +14  |
| _"Two tools change things (Xdebug…, reissuing the certificate)"_ (satır 139) | **17 araçtan 7'si** `writes: true` | +5   |

Yedi yazma aracı: `xdebug_set`, `certificates_reissue`, `project_start`,
`project_stop`, `stack_up`, `stack_down`, `generate`. README yalnızca ilk
ikisini sayıyor — yani `--allow-writes` bayrağının bir MCP istemcisine
verdiği yetkinin **stack'i tümüyle durdurmayı içerdiği** dokümantasyonda
yazmıyor. Bu bir _güvenlik dokümantasyonu_ boşluğu, tipografik bir hata değil.

**"Yerine bu olsaydı."** Bu sayılar prose'da olmamalıydı. Ya üretilmeli
(`mcp.rs` zaten `TOOLS`'u tarayan testlere sahip — bir tanesi de README
tablosunu üretebilirdi), ya da hiç sayı verilmemeliydi. Bir doğrulama kültürü
kuran projede, doğrulanmayan tek metnin README olması ironik ve düzeltilebilir.

---

## 12. Yönetişim ve süreklilik

**Ölçüm.** `git log` → 21 commit, **1 yazar**. `CODEOWNERS` → her satır aynı
kişi, yani her PR'ı kendisi onaylıyor.

Bu bir eleştiri değil, bir **risk beyanı**: bugün bu projeyi devralacak ikinci
kişi için giriş noktası yok.

- **`ARCHITECTURE.md` yok.** 35k satır Rust, 49 modül. README ürünü anlatıyor,
  mimariyi değil. "Bir komut çağrıldığında ne olur" akışı hiçbir yerde yok.
- **ADR yok.** Kararlar kod yorumlarında — mükemmel yazılmış ama
  **greplenemez, numaralanamaz, üstüne yazılamaz**. `elevate.rs`'in başındaki
  mkcert anlatısı bir ADR'dir; `docs/adr/0007-elevation.md` olsaydı hem
  bulunur hem de "bu karar 2027'de şu yüzden değişti" diye devam ettirilebilirdi.
- **CI kapısı harici repoya bağımlı** (§2.1) — bugün çalışıyor, garantisi yok.
- **Breaking-change politikası yazılı değil.** `contractVersion` alanı var (iyi),
  ama neyin major sayıldığı tanımsız.

---

## 13. Gerçek anlamda "kurumsal" olan ve tamamen eksik olanlar

Bir geliştiricinin makinesinden bir kurumun filosuna geçişte sorulanlar. Bugün
hiçbirinin cevabı yok.

| İhtiyaç                       | Bugün             | Olması gereken                                                                                                 |
| ----------------------------- | ----------------- | -------------------------------------------------------------------------------------------------------------- |
| Merkezî konfigürasyon         | yok               | MDM / Group Policy / `/Library/Managed Preferences` okuyan, kullanıcı ayarını override eden politika katmanı   |
| Zorunlu/kilitli ayarlar       | yok               | "Güncelleme kanalı kilitli", "telemetri kapalı", "workspace şurada"                                            |
| Private Docker registry       | yok               | Şablonlardaki `image:` referansları için kurumsal mirror ön eki                                                |
| macOS sistem proxy'si         | okunmuyor (§5.4)  | `macos-system-configuration` özelliği + görünür hata                                                           |
| Air-gapped kurulum            | yok               | Şablonlar zaten binary'de; imajlar Docker Hub'dan, offline bundle yolu yok                                     |
| Denetim izi                   | kısmi             | `/etc/hosts` değişikliği, container silme, sertifika yenileme — ayrı, yapılandırılmış, döndürülmeyen audit log |
| Üçüncü taraf lisans bildirimi | yok               | About kutusunda / NOTICE dosyasında bağımlılık lisansları — MIT dağıtım yükümlülüğü                            |
| Erişilebilirlik beyanı        | yok               | VPAT / EN 301 549                                                                                              |
| Gizlilik beyanı               | yok               | Hangi veri nerede, ne kadar kalıyor                                                                            |
| Destek / sürüm ömrü           | "yalnızca en son" | LTS ya da en az N-1 backport politikası                                                                        |

---

## 14. Öncelik sırası

Etki/maliyet oranına göre. İlk yedisi bir haftalık iş.

### Şimdi (gün–hafta) — **uygulandı, §17'ye bakınız**

1. ✅ **Panic hook + crash dosyası** (§4.1) — ~15 satır. `panic = "abort"` ile
   bugün her çökme izsiz. **Tek en yüksek getirili düzeltme.**
2. ⚠️ **Release blokajlarını kaldır** (§6.1) — anahtar çifti üret **ve**
   endpoint'i ayağa kaldır. İkisi ayrı işler. Anahtar üretildi ve `pubkey`
   dolduruldu; **endpoint hâlâ 404** ve açık blokaj olarak duruyor.
3. ✅ **SECURITY.md'deki 404 advisory linkini düzelt** (§6.1) — güvenlik bildirim
   yolunun kırık olması tek satırlık ama ciddi bir kusur.
4. ✅ **README'deki iki yanlış sayıyı düzelt** (§11) — özellikle
   `--allow-writes`'ın 7 aracı açtığı; bu bir güvenlik dokümanı satırı.
5. ✅ **Kapsam ölçümünü aç** (§3.1) — `cargo llvm-cov` + `vitest --coverage`,
   eşiksiz başla, sadece **gör**.
6. ✅ **Sürüm numarası eşitlik testi** (§6.2) ve **macOS imzasız-build uyarısı**
   (§6.4) — ikisi de birkaç satır.
7. ✅ **`elevate` quoting'i düzelt** (§5.1) — `osascript … on run argv`.
8. ✅ **`macos-system-configuration` özelliğini ekle** (§5.4) — tek satır.
   _(Özelliğin adı bu sürümde farklı çıktı; §17.2'ye bakınız.)_

### Sonraki çeyrek (hafta–ay)

- **9.** **`ProgressSink` trait'i + `stackvo-core` crate'i** (§2.1) — en yüksek
  getirili mimari değişiklik. 48 komutun bağımlılığı, MCP'nin kapsamı, komut
  testlerinin tamamı ve gelecekteki CLI bunun arkasında.
- **10.** **`tauri-specta` ile tip üretimi** (§2.2) — `ipc.js` ve suite E
  ortadan kalkar, frontend tipli olur.
- **11.** **`hint` metinlerini kod tabanlı i18n'e taşı** (§7.1) — kullanıcının
  eyleme geçtiği metin.
- **12.** **`tauri-driver` ile 5 E2E senaryosu** (§3.2).
- **13.** **SBOM + build provenance** (§5.3) — CI'da ~10 satır.
- **14.** **Tanılama paketi düğmesi** (§4.2) ve **`vitest-axe`** (§8).
- **15.** **Bozuk prefs'i yedekle + `schemaVersion`** (§10).

### Yapısal (çeyrek+)

- **16.** **Settings.vue ve ProjectDetail.vue'yu böl** (§2.3) — sekme başına
  bileşen + composable + **mount edilen** test; "shape mirror" testlerini
  emekliye ayır.
- **17.** **`ARCHITECTURE.md` + `docs/adr/`** (§12) — mevcut yorumları taşıyarak
  başla; yeni yazı gerekmiyor, yalnızca yer değiştirme.
- **18.** **Merkezî politika katmanı** (§13) ve **private registry ön eki**.
- **19.** **Docker'ı trait arkasına al** (§3.3), **`proptest`** (§3.4),
  **`criterion` + `size-limit`** (§9).
- **20.** **Keystore ile sır yönetimi** (§5.2) — v2 sözleşme değişikliği olarak
  planla.

---

## 15. İlk taslakta yanlış olan ve düzeltilenler

Bu bölüm, dokümanın kendi hata payının kaydı. Dördü de ölçülerek yakalandı.

| İlk taslak iddiası                                                        | Gerçek                                                                                                                                                                                                                     | Nasıl yakalandı                                               |
| ------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| _"rustls sistem trust store'unu kullanmıyor; kurumsal MITM CA çalışmaz."_ | **Yanlış.** `rustls-platform-verifier 0.7.0` graf içinde; macOS `security-framework`, Windows `windows-sys`, Linux `rustls-native-certs`. Sistem trust store kullanılıyor. Gerçek boşluk yalnızca macOS sistem _proxy'si_. | `cargo tree -e features -i reqwest`                           |
| _"Rust hata mesajları hiç çevrilmiyor; kullanıcı İngilizce hata okuyor."_ | **Kısmen yanlış.** 12 hata kodunun **tamamı** + `UNKNOWN` çevrili ve `ErrorAlert.vue` bunu başlık olarak gösteriyor. Boşluk yalnızca spesifik mesaj ve `hint`.                                                             | `en.js:1342` `errors` bloğu okundu                            |
| _"Üretim kodunda 364 `unwrap/expect` var."_                               | **Yanlış** — o sayı test modüllerini içeriyordu. `#[cfg(test)]` öncesi bölümde toplam **7**. Bu bir kusur değil, projenin güçlü yanı.                                                                                      | Dosya başına `#[cfg(test)]` satır numarasına kadar sayım      |
| _"MCP 34 komuta ulaşamıyor"_ (README'den alınmıştı)                       | **README'nin kendisi yanlış.** Ölçüm: **48** komut `AppHandle` alıyor. README'nin ikinci sayısı da yanlış (§11).                                                                                                           | `commands.rs` imza taraması                                   |
| _"56 hata mesajı, 46 hint çevrilmiyor."_                                  | **İkisi de yanlıştı** — ilk sayım test modüllerini içeriyor ve `format!` ile kurulan mesajları atlıyordu. Doğrusu: **113** mesaj, **33** hint.                                                                             | `#[cfg(test)]` öncesi bölümde ifade tipine göre sınıflandırma |

Ayrıca ilk taslakta **olmayan**, doğrulama sırasında ortaya çıkan üç bulgu:
güncelleme endpoint'inin 404 vermesi (§6.1), SECURITY.md advisory linkinin ölü
olması (§6.1), ve README'nin MCP yazma araçlarını 2 olarak sayarken gerçekte 7
olması (§11).

---

## 16. Kapanış

Bu kod tabanının sorunu kalite değil. `atomic.rs`, `inflight.rs`, `git.rs`,
`quickcmd.rs` ve `contracts/ipc.json`'un kendi istisnalarını makine-okunur
alanlarla işaretlemesi — beşi de, çoğu ekibin hiç yazmadığı problemleri doğru
çözmüş ve **neden** öyle çözdüğünü yazmış. Doğrulama sırasında dört iddiamı
bozan şey de buydu: kod, ilk bakışta göründüğünden daha doğruydu.

Sorun **devredilebilirlik**. Bugün bu projedeki doğruluğun büyük kısmı bir
kişinin dikkatiyle korunuyor: 143 komutun kaydı elle, IPC tipleri elle, hata
önerileri elle, kapsam ölçülmemiş, E2E yok, panic izsiz, mimari kararlar
yorumlarda, ve dokümantasyonun kendisi — projenin tüm doğrulama kültürüne
rağmen — doğrulanmıyor.

Bunların hepsi tek bir kişi her satıra bakarken çalışır. İkinci geliştirici
geldiği gün ya da altıncı ayda hafıza soluklaştığında çalışmaz.

Kurumsal seviye, daha fazla özellik değil; **kalitenin insandan bağımsız hale
gelmesidir.** §14'teki ilk sekiz madde bir haftalık iş ve bu dönüşümün
başlangıcı.

---

## 17. Uygulama kaydı — §14 "Şimdi" grubu

§14'ün ilk sekiz maddesi uygulandı. Bu bölüm ne yapıldığını, **ve raporun kendi
iki hatasını** kaydediyor — §15'in aynı gerekçesiyle: bir denetim raporunun
uygulama sırasında yanlış çıkan tavsiyesini gizlemesi, onu bir daha okunmaz
yapar.

### 17.1 Yapılanlar

|   # | Madde                             | Ne yapıldı                                                                                                                                                                                                                        |
| --: | --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|   1 | Panic hook (§4.1)                 | Yeni `crash.rs`: `set_hook` + `crash-<UTC>-<pid>.txt`, senkron `fs::write` ile. Mesaj `logging::redact`'ten geçiyor. Son 10 rapor tutuluyor. Hem app hem `stackvo-mcp` kuruyor. 9 test.                                           |
|   3 | SECURITY.md (§6.1)                | Advisory linki `stackvo/stackvo`'ya alındı — doğrulandı, HTTP 200.                                                                                                                                                                |
|   4 | README sayıları (§11)             | 34 → **48**, "iki araç" → **yedi araç, adlarıyla**. Yeni `tests/readme_claims.rs` ikisini de koda karşı ölçüyor: yanlış sayı da, eksik araç adı da build'i kırıyor (kırıldığı doğrulandı).                                        |
|   5 | Kapsam (§3.1)                     | `vitest --coverage` (v8) + CI'da `cargo llvm-cov`. Eşik **yok**, run summary'ye rapor.                                                                                                                                            |
|   6 | Sürüm + imza uyarısı (§6.2, §6.4) | `tests/version_agreement.rs` üç dosyayı eşitliyor. `release.yml` artık macOS için de imzasız/notarize-edilmemiş durumu uyarıyor — dört senaryonun dördü de koşturularak doğrulandı.                                               |
|   7 | `elevate` (§5.1)                  | Raporun "doğrusu" seçeneği: `shell(&str)` → `run(&[&str])`. Script sabit, yollar `argv` ile gidiyor, `quoted form of` kaçışlıyor. **İnterpolasyon kalmadı.** 6 test — üçü gerçek `osascript` çalıştırıp düşmanca girdiyi deniyor. |
|   8 | Sistem proxy (§5.4)               | `reqwest`'e `system-proxy` + `mail.rs`'e `no_proxy()` istemci.                                                                                                                                                                    |
|   2 | Release (§6.1)                    | İmza anahtarı üretildi, `pubkey` dolduruldu — `release.yml` preflight artık geçiyor. **Endpoint hâlâ 404: açık blokaj.**                                                                                                          |

### 17.2 Raporun uygulama sırasında yanlış çıkan iki tavsiyesi

| Rapordaki iddia                                                                   | Gerçek                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | Nasıl yakalandı                                                                     |
| --------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| _"`macos-system-configuration` özelliğini eklemek (tek satır)"_ (§5.4)            | **Özellik adı yanlış.** O ad `reqwest` 0.12'nin; bu repo 0.13.4 kullanıyor ve orada adı **`system-proxy`** (ve `default`'un parçası, `default-features = false` ile kapanıyor). Ayrıca **tek satır değil**: hyper-util'in macOS okuyucusu sistemin istisna listesini ve "Exclude simple hostnames"i okumuyor, `NO_PROXY` dışında hiçbir şey daraltmıyor. Özellik süreç geneli olduğu için `mail.rs`'in `127.0.0.1` trafiği kurumsal proxy'ye düşerdi — yani özellik, tam da açıldığı makinede mail catcher'ı bozardı. `mail::client` artık `no_proxy()` ile kuruluyor. | `reqwest-0.13.4/Cargo.toml` özellik listesi + `hyper-util`'in `matcher.rs`'i okundu |
| _"`scaffold.rs` … 791 satır, 5 test"_ — sıcak ve zayıf modüller listesinde (§3.1) | **Zayıf değil: %94.09 satır kapsamı.** Test _yoğunluğu_ kapsamı yanlış tahmin ediyor; §3.1'in kendi tezi bu tabloyla çürüdü. Aynı tabloda `error.rs` (%30.65), `engine.rs` (%19.65), `pty.rs` (%29.04), `watcher.rs` (%43.62) doğru çıktı.                                                                                                                                                                                                                                                                                                                             | `cargo llvm-cov --summary-only`                                                     |

### 17.3 Ölçüm artık var: ilk sayılar

Raporun §3.1'de "bilinmiyor" dediği şey artık bir sayı.

|                       | Satır kapsamı |
| --------------------- | ------------: |
| **Rust** (toplam)     |    **%61.60** |
| `generator.rs`        |        %94.89 |
| `scaffold.rs`         |        %94.09 |
| `migrate.rs`          |        %82.46 |
| `phpini.rs`           |        %67.26 |
| `watcher.rs`          |        %43.62 |
| `db.rs`               |        %35.14 |
| `error.rs`            |        %30.65 |
| `pty.rs`              |        %29.04 |
| `engine.rs`           |        %19.65 |
| **`commands.rs`**     |    **%18.18** |
| **Frontend** (toplam) |    **%30.70** |
| `src/lib/**`          |        %91.42 |
| `src/stores/**`       |        %78.87 |
| **`src/views/**`**    |        **%0** |

İki sayı raporun iki ayrı bölümünü sayıya çeviriyor:

- **`commands.rs` %18.18** — 3.128 satır hiç çalıştırılmıyor. §2.1'in "tanrı
  modül" teşhisi bir mimari tercih değil, ölçülebilir bir test boşluğu: 48
  komutun `AppHandle`'a yapışık olması onları test edilemez yapıyor, ve
  `ProgressSink` (§14.9) bu sayının önündeki tek engel.
- **`src/views/**` %0** — `Settings.vue`'nun 3.172 satırının, `ProjectDetail.vue`'nun
  2.712 satırının **hiçbiri** koşulmuyor. §2.3'ün "shape mirror testleri davranışı
  kopyada doğruluyor, üründe değil" tespitinin tam sayısal karşılığı: 16 test
  dosyası, 160 test, ve ürün bileşenlerinden geçen sıfır satır.

### 17.4 Kod tarafında açık kalanlar

1. **Kapsam eşiği yok.** §14.5 bilinçli olarak "eşiksiz başla" diyordu. Sayılar
   artık elde; eşik ayrı ve bilgilendirilmiş bir karar.

### 17.5 Kodun çözemeyeceği, sahibine kalanlar

Aşağıdakiler bir commit'le kapanmıyor: üçü bir hesabın sahibi olmayı, biri de
bir politika kararı vermeyi gerektiriyor. **Bu bölüm, raporun §12'de "bus factor
1" dediği şeyin somut hâlidir** — hepsi tek bir kişinin elinde ve hiçbirinin
başka bir yerde kaydı yok.

|   # | Ne                                       | Neden devredilemiyor                                                                                                                                                                                                                                                              | Bugünkü etkisi                                                                                                   |
| --: | ---------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
|   1 | **Güncelleme endpoint'i 404**            | `tauri.conf.json` `stackvo/stackvo-tauri`'yi gösteriyor; o repo **yok** (doğrulandı: HTTP 404). Nerede yayınlanacağı bir sahiplik kararı — `stackvo/stackvo` release'leri mi, yeni bir repo mu.                                                                                   | İmza tarafı çözüldü, dağıtım tarafı çözülmedi: **uygulama hâlâ güncelleme alamaz.** Blokajın _ikinci_ yarısı bu. |
|   2 | **`TAURI_SIGNING_PRIVATE_KEY` secret'ı** | Özel anahtar üretildi ve `~/.tauri/stackvo.key`'de duruyor (mod 600); public yarısı `tauri.conf.json`'a girdi. Özel yarı **repoya girmedi ve girmemeli** — GitHub repository secret'ı olarak eklenmesi gerekiyor. Parolasız üretildi; parolalı istenirse çift yeniden üretilmeli. | `release.yml` preflight'ının pubkey kontrolü artık geçiyor, secret kontrolü hâlâ bloke ediyor — doğru davranış.  |
|   3 | **Apple / Windows imzalama secret'ları** | `APPLE_CERTIFICATE`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`, `WINDOWS_CERTIFICATE`. Hepsi ücretli geliştirici hesaplarına bağlı.                                                                                                                 | §6.4'ten sonra artık **sessiz değil**: eksikse release log'unda uyarı çıkıyor. Ama hâlâ eksikler.                |
|   4 | **Kapsam eşiği**                         | Sayılar artık var (§17.3). %61.60'ı mı yoksa daha düşük bir tabanı mı kilitleyeceği mühendislik değil, politika kararı.                                                                                                                                                           | Ölçüm var, kapı yok.                                                                                             |

Ayrıca, **bu çalışmadan bağımsız** ve HEAD'de de mevcut olan iki kırık
(`git stash` ile doğrulandı):

- **`npm run lint` exit 1** — dört dump dosyası Prettier'dan geçmiyor, yani
  CI'ın "Lint the front end" adımı bugün main'de kırmızı. `npm run lint:fix`
  tek komutluk düzeltme.
- **`preflight::tests::a_fresh_install_asks_for_the_two_core_names_and_nothing_else`
  düşüyor** — `["phpmyadmin.example.test", "rabbitmq.example.test"]`.

---

## 18. Uygulama kaydı — "Sonraki çeyrek" grubu, ilk tur

§14'ün 9–15 arası maddelerinden üçü tamamlandı, biri ölçülüp ertelendi.

### 18.1 §14.9 — `ProgressSink`: iki dilim

Rapor bunu "projedeki en yüksek getirili tek değişiklik" diye işaretlemişti ve
bir şeyi atlamıştı: **`events::Sink` zaten vardı.** İki varyantlı bir enum —
`App(AppHandle)` ve `Headless` — ve `runner::run_operation` zaten onu alıyordu.
Yani ayrıştırmanın yarısı yapılmıştı; rapor bunu görmeden sıfırdan öneri yazdı.

Eksik olan yarısı şuydu: **enum'un gözlenebilir bir üçüncü varyantı yoktu.**
`Sink::App` çalışan bir Tauri uygulaması istiyor, `Sink::Headless` her şeyi
atıyor. Üçüncü cevap — "topla, sonra iddia et" — testlerin ihtiyaç duyduğu tek
cevaptı ve yoktu. Sonucu ölçülebilir: `run_operation`, uygulamadaki **her uzun
işlemin** geçtiği huni (11 komut, her compose çalıştırması, her build, her
clone), **hiç testi yoktu.** Yazılmamış değil; _yazılamazdı_.

**Dilim 1.** Yeni `progress.rs` — içinde tek bir `use tauri::` yok:

|                      |                                                                                                                                                      |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `trait ProgressSink` | `fn event(&self, name: &str, payload: Value)`. `dyn`-uyumlu olması için jenerik değil; payload `Value`, çünkü zaten webview'e JSON olarak gidiyordu. |
| `Null`               | Pencere yok, olaylar düşer. `stackvo-mcp` artık `Sink::Headless` yerine bunu kullanıyor — MCP yolu artık hiçbir Tauri tipi adlandırmıyor.            |
| `Recording`          | Var olmayan implementasyon. Olayları sırasıyla tutar; `names()`, `named()`, `last()`.                                                                |

`events::Sink` trait'i implement ediyor, `run_operation` artık
`&dyn ProgressSink` alıyor — masaüstü çağrı yerleri değişmedi (`&Sink`
otomatik `&dyn`'e dönüşüyor), webview aynı JSON'u alıyor.

Sonra `run_operation`'ın dört dalı da test edildi: başarı (satır başına progress

- tek terminal olay), sıfır-olmayan çıkış (hem `Err` **hem** başarısız terminal
  olay — birini emitip diğerini atlamak konsolu sonsuza kadar döndürür), başlamayan
  program, ve pencere olmadan aynı sonuç. **`runner.rs`: %98.17.**

**Dilim 2.** `generate()` `AppHandle`'ı iki ilgisiz sebeple alıyordu: yönetilen
kilit ve sink. İkincisi ayrıldı — `generate_reported(&dyn ProgressSink, …)` —
ve olay sözleşmesi ilk kez test edildi: dosya başına bir `generate:progress`,
sonra tam olarak bir `generate:done`. Başarısızlık yolunda da terminal olayın
geldiği ayrıca doğrulandı; `Err` dönüp olayı atlamak hiçbir tipin yakalamadığı
bir hata ve konsolu asla bitmeyen bir işlemde bırakıyor.

### 18.2 §14.15 — bozuk tercih dosyası, ve raporun görmediği ikinci hata

Rapor birini yakalamıştı: `unwrap_or_else(|_| default_prefs())` çökmüyordu ama
kullanıcının **tüm ayarlarını** uyarısız varsayılana döndürüyor ve bozuk dosyayı
yedeklemiyordu — sonraki `prefs_set` de kanıtın üzerine yazıyordu.

**Görmediği ikincisi daha sinsiydi.** `serde_json::from_str` bir `3`'ü, bir
`"dark"`'ı ya da bir diziyi _geçerli JSON_ olarak kabul eder ve eski kod onu
öylece döndürüyordu. Sonrasında her `prefs_set` çağrısı `as_object_mut()`'tan
`None` alıyor, shallow merge hiçbir şey yapmıyor, ve aynı skaler geri
yazılıyordu. Yani: **kullanıcı ayarları değiştiriyor, hiçbiri kaydedilmiyor,
diskte geçerli bir dosya var ve hiçbir yerde hata yok.**

Yapılan: `schemaVersion: 1` (ileride yeniden adlandırılacak bir anahtar için tek
tutamak), nesne olmayan JSON de bozuk sayılıyor, ve bozuk dosya
**kopyalanmıyor — taşınıyor** (`preferences.corrupt-<UTC>.json`). Taşımak
kasıtlı: kopyalasaydık bozuk dosya orada durduğu sürece her açılışta yeni bir
yedek üretilirdi. Yalnızca _bozuk_ JSON'da çalıştığı için güvenli — daha yeni bir
sürümün bilinmeyen anahtarlar taşıyan dosyası hâlâ geçerli bir nesnedir, okunur
ve karantinaya alınmaz (bu da ayrıca test edildi).

### 18.3 §14.13 — SBOM, provenance, checksum

Üçü de eklendi ve **yerelde çalıştırılarak** doğrulandı:

- **SBOM, iki dosya.** `cargo cyclonedx` (380 bileşen) + `npm sbom` (16, prod).
  Tek dilli bir SBOM, Tauri uygulamasının yarısını atlayan bir belgedir.
- **Build provenance** — `actions/attest-build-provenance`, artı iş için
  gereken `id-token: write` ve `attestations: write` izinleri.
- **SHA-256 listesi** — `latest.json` imzalı olduğu için _updater_ kanıtlayabiliyordu;
  releases sayfasından elle indiren kişinin hiçbir yolu yoktu.

### 18.4 §14.10 — ölçüldü, ertelendi

Rapor bunu "`ipc.js` ve suite E ortadan kalkar" diye tek maddede geçiyor. Ölçüm:

- **59 farklı dönüş tipi**, ~40'ı özel struct — hepsine `specta::Type` gerekiyor.
- **143 fonksiyona** `#[specta::specta]`, artı argüman tipleri.
- Özel `error::Error`/`Result` de `Type` implement etmeli.
- Ve **15 komut `serde_json::Value` döndürüyor** — bunlar üretimden sonra da
  tipsiz kalır, yani raporun vaat ettiği "frontend tipli olur" kısmen gerçekleşir.

Bu bir günlük iş değil ve yarım inen bir tip üretimi, elle yazılan `ipc.js`'ten
daha kötüdür: iki kaynak yerine üç kaynak olur. **Ayrı bir dal olarak
planlanmalı.** Bu arada §2.1'in asıl riski — sözleşme kapısının harici bir repo
checkout'una bağlı olması — ondan bağımsız ve çok daha ucuz kapatılabilir.

### 18.5 Kapsam, iki tur sonra

|                 |                       Başlangıç |      Şimdi |
| --------------- | ------------------------------: | ---------: |
| **Rust toplam** |                          %61.60 | **%63.12** |
| `runner.rs`     | (`run_operation` test edilemez) | **%98.17** |
| `commands.rs`   |                          %18.18 | **%23.97** |
| `progress.rs`   |                               — |     %98.06 |

Rust testleri: **448 → 469.**

### 18.6 Kendi kapısına takılan test

§17'de eklenen `readme_claims.rs`, bu turda **kendi eklediğim koda takıldı** —
ve doğru sebeple.

`commands.rs`'te komutları sayarken test modüllerini dışlamak için brace
sayıyordu. §18.2'nin testlerinden biri kasten bozuk JSON yazıyor:
`"{\"theme\": \"dark\", trunca"` — bir string literali içinde **kapanmayan bir
`{`**. Sayaç bir daha sıfıra dönmedi, son test modülü "test modülü" olarak
tanınmaz oldu, ve yalnızca testlerde var olan 3 komut üretim yüzeyi gibi
sayılmaya başladı: **143 yerine 146**.

Testin yakaladığı şey README değildi; **kendi ölçümünün güvenilmez hâle
geldiğiydi**:

> `the command scan found 146 commands, so its count of 48 AppHandle commands
cannot be trusted either`

O `assert_eq!(total, 143)` satırı savunma amaçlı yazılmıştı ve tam olarak
öngörüldüğü şekilde işledi — yanlış bir sayı README'ye sessizce yerleşmek yerine
build'i kırdı.

Düzeltme, string'leri ayrıştırmak değil; o bir Rust lexer'ı yazmak demek. Bunun
yerine CI'ın zaten dayattığı bir invariant kullanıldı: `cargo fmt --check` her
push'ta koşuyor ve rustfmt üst seviye bir öğeyi **sıfırıncı sütundaki** bir `}`
ile kapatıyor. Hiçbir string literali buna benzeyemez, çünkü rustfmt sahip
olduğu her satırı girintiler.

Kayda değer olan: bu, raporun §11'de savunduğu şeyin çalışan hâli. Bir ölçümün
kendi geçerliliğini de iddia etmesi, ölçümün yanlış olduğu günü fark edilir
kılıyor.

---

## 19. Kalanları toplama turu

Bu tur yeni bir madde açmadan önce, önceki turların bıraktıklarını kapattı.
Dördü "önceden vardı, bu çalışmayla ilgisi yok" diye kaydedilmişti; kaydedilmiş
olmaları çözülmüş olmaları değil.

### 19.1 CI'ın kendi kapıları

| Ne                                    | Neydi                                                                                                                                                                                           | Ne yapıldı                                                                                                  |
| ------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `npm run lint` exit 1                 | Dört dump dosyası Prettier'dan geçmiyordu — yani "Lint the front end" adımı main'de kırmızıydı.                                                                                                 | `prettier --write`. Dördü de saf biçimlendirme; diff'i satır satır kontrol edildi, anlamsal değişiklik yok. |
| ESLint `coverage/` dizinini tarıyordu | **Bu turun kendi regresyonu.** §17.5'te açılan kapsam raporu, v8 reporter'ın kendi HTML paketini üretiyor; ESLint onun `eslint-disable` yorumlarını "kullanılmayan direktif" diye raporluyordu. | `eslint.config.js` ignore listesine `coverage/**`.                                                          |

### 19.2 Hermetik olmayan test

`preflight::tests::a_fresh_install_asks_for_the_two_core_names_and_nothing_else`
düşüyordu ve HEAD'de de düşüyordu — o yüzden "pre-existing" diye kaydedilmişti.
Sebebi kaydedilmemişti, ve sebep kodda değildi.

Test `missing_hosts_by_owner`'ı çağırıyor; o zincir **gerçek Docker daemon'ına**
ve **gerçek `/etc/hosts`'a** ulaşıyor. Testin kendi yorumu şöyle diyordu:

> _Nothing here starts Docker, and that is the point: `stackvo_containers`
> fails, nothing is running…_

Doğru, ve aynı şey değil. Docker'ı _başlatmamak_ ile hiçbir şeyin _çalışmıyor
olması_ farklı iddialar. CI runner'ında ikisi çakışıyor, o yüzden test yeşildi.
Stack'i fiilen çalıştıran bir geliştirici makinesinde phpMyAdmin ve RabbitMQ
**çalışıyor**, kod onları doğru şekilde listeliyor, ve test kodda olmayan bir
hatayı bildiriyordu. **Yani: bakımcının kendi makinesinde koşamayan bir test.**

`service_domains`'in iki ortam okuması (çalışan container'lar, `/etc/hosts`)
argümana çevrildi. Kural artık _belirtilen_ bir dünyaya karşı doğrulanıyor:
hiçbir şey çalışmıyor **ve** hiçbir şey yazılı değil — "fresh install" tam olarak
budur. Bir de tersi eklendi: bir servis çalışıyorsa adı isteniyor. Sadece boş
durumu doğrulamak, her zaman boş dönen bir fonksiyondan da geçerdi.

### 19.3 Dört "unhandled error" — tipsiz IPC sınırının canlı hâli

Vitest 4 unhandled rejection basıyordu ve testler yine de geçiyordu, o yüzden
kimse peşine düşmemişti. Sebebi §2.2'nin tam olarak tarif ettiği şeydi:

`inventory.js`, `projects.value = await api.projectsList()` diye yazıyordu. Sınır
tipsiz — `ipc.js` elle yazılmış, hiçbir şey bir Rust komutunun hâlâ beklenen
şekli döndürdüğünü kontrol etmiyor. Bir `null` geldiğinde (adı değişmiş bir alan,
`deferred` bir komut, `None` dönen bir `Option`) her `computed` `null` üzerinde
`.filter` okuyor ve **render fırlatıyor**. Bir masaüstü uygulamasında bu eksik
bir liste değil, **boş bir pencere**.

Sınır artık güvenilmez muamelesi görüyor (`asList`), ve 13 testlik yeni bir
dosya bunu beş farklı bozuk cevaba karşı sabitliyor — artı iyi verinin
dokunulmadan geçtiğini, çünkü sessizce boşaltan bir koruma değiştirdiği
çökmeden kötüdür.

### 19.4 §14.9'un kalanı: `lifecycle`

Altı start/stop/restart komutunun paylaştığı gövde `&AppHandle` alıyordu; artık
`&dyn ProgressSink` alıyor. Kazanç doğrudan test edilebilirlik: gövdenin ilk işi
kabul etmediği bir adı reddetmek, ve **o kapı hiç denenmemişti** — çünkü ona
ulaşmak çalışan bir Tauri uygulaması gerektiriyordu.

Yorumuna güvenilmek yerine test edilmeyi hak ediyor, çünkü id id olarak kalmıyor:
container adı ve compose servis adı oluyor, ve aşağıda hiçbir şey onu yeniden
kontrol etmiyor. Altı düşmanca ad artık reddediliyor **ve** reddedilmeden önce
UI'a hiçbir olay gitmediği doğrulanıyor.

_(Bu arada bir beklentim yanlış çıktı: bilinmeyen servis `InvalidInput` değil
`NotFound` dönüyor. Kod haklı — ad iyi biçimli, sadece hiçbir şeyi
adlandırmıyor — ve ikisi kullanıcıya farklı çevrilmiş başlık olarak ulaştığı için
hangisi olduğu davranıştır. Test gerçeğe uyduruldu.)_

### 19.5 §14.14'ün ilk yarısı: axe

`vitest-axe` eklendi ve **ilk çalıştırmasında iki gerçek ihlal buldu**:

- **`StatCard`** — Vuetify'ın `v-progress-linear`'ı `role="progressbar"` ve
  `aria-valuenow` üretiyor, ad üretmiyor. Dashboard'da dört tane yan yana duruyor,
  yani ekran okuyucu neyin ne olduğunu söylemeden dört çıplak sayı okuyordu.
- **`BootstrapGate`** — aynı boşluk, ve **ilk açılış ekranında**. `RequirementsGate`
  bunu zaten doğru yapmıştı, yani üç barın ikisi eksikti.

İkisi de kaynağa bakarak görülmüyor; Vuetify'ın ne ürettiğini bilmeyi
gerektiriyor. Tam olarak bir makinenin insandan iyi olduğu sınıf.

**Ve bir şey kapatıldı: `color-contrast`.** jsdom'da canvas yok, axe kontrastı
canvas'a boyayarak ölçüyor — kural açık bırakılsaydı her bileşende sonsuza kadar
_hiçbir şey kontrol etmeden_ geçerdi. Hiç koşmamasından kötü olurdu: vermediği bir
garantiyi veriyormuş gibi görünen yeşil bir suite. Üstelik bu uygulamanın en çok
ihtiyaç duyduğu kural o — `appearance.js` temayı işletim sisteminin vurgu
renginden türetiyor, yani palet sabit değil ve elle bir kez denetlenemez. Gerçek
bir tarayıcı gerekiyor: §14.12.

### 19.6 Sayılar

|                     |  Tur 1 |  Tur 2 |      Şimdi |
| ------------------- | -----: | -----: | ---------: |
| **Rust toplam**     | %61.60 | %63.12 | **%63.34** |
| `commands.rs`       | %18.18 | %23.97 | **%25.46** |
| **Frontend toplam** | %30.70 | %30.70 | **%31.44** |
| Rust testleri       |    448 |    469 |    **472** |
| Frontend testleri   |    160 |    160 |    **182** |

Ve ilk kez: **Rust paketi tamamen yeşil**, `npm run lint` exit 0, vitest'te
sıfır unhandled error.

### 19.7 Hâlâ kalan

- **§14.14'ün ikinci yarısı — tanılama paketi (§4.2).** Yeni bir IPC komutu,
  sözleşme girdisi, `ipc.js` sarmalayıcısı, bir zip bağımlılığı ve Settings'te
  bir düğme. Dikey bir dilim; yarım inmesi işe yaramaz.
- **§14.11 — `hint` metinlerini i18n'e taşımak.** 33 çağrı yeri, `Error`'a bir
  `hint_key` alanı, locale dosyaları ve `ErrorAlert.vue`. Sözleşme değişikliği.
- **§14.12 — E2E.** §19.5'in kontrast kuralının beklediği şey.

---

## 20. §14.11 — hint metinleri i18n'e

### 20.1 Raporun sayısı yanlıştı

§7.1 **33** `with_hint` sayıyordu. Ölçüm: **60** — 57'si düz literal, 3'ü
çalışma anında kurulan. 56 tanesi de birbirinden farklı.

_(Aradaki fark muhtemelen §15'te kaydedilen `format!` sorununun aynısı: ilk
sayım çok satırlı literal'leri ve `.to_string()` ile yazılmış olanları
atlıyordu. Aynı hatanın hem mesaj hem hint sayımında tekrarlanması, "ifade
tipine göre sınıflandırma"nın tek seferlik bir düzeltme değil, sürekli bir
yöntem sorunu olduğunu gösteriyor.)_

### 20.2 Neden anahtar değil, katalog

Raporun önerisi `Error`'a bir `hint_key` taşıtmaktı. Doğru, ama tek başına
uygulandığında İngilizce metin yine 25 dosyada kalır ve anahtar 25 yerde daha
yazılır — yani yazım hatası yüzeyi ikiye katlanır.

Bunun yerine `src/hints.rs`: her hint **bir kez**, anahtarı ve İngilizcesiyle
birlikte tanımlanıyor, çağrı yerleri ada referans veriyor:

```rust
Err(Error::new(Code::EngineUnreachable, "…").with_hint(hints::START_DOCKER))
```

Üç kazanç: çağrı yeri değiştirdiği string'den daha okunur, yanlış ad derleyici
hatası, ve raporun asıl istediği şey — **tüm küme tek dosyada gözden
geçirilebilir**. Bir `hints!` makrosu her sabiti otomatik olarak `ALL`
dizisine yazıyor; kaydolmayan bir hint çeviri testine görünmez olurdu, ki bu da
"hint'leri çeviriyoruz" ile "hatırlanan hint'leri çeviriyoruz" arasındaki fark.

`with_hint` hem `Hint` hem düz `String` kabul ediyor. Çalışma anında kurulan üç
hint (program adı, git hatası) anahtarsız gidiyor ve İngilizce kalıyor — daha
önce **hepsi** öyleydi, yani bu bir gerileme değil, kapsanmayan bir kalıntı.

### 20.3 İngilizce hiçbir yerden kalkmadı

`Error.hint` hâlâ İngilizceyi taşıyor. Log onu yazıyor, MCP istemcisi onu
görüyor, ve locale anahtarı bulamazsa arayüz ona düşüyor. Çeviri, mevcut
davranışa **eklendi**; yerine geçmedi. Sözleşmeye `hintKey` alanı bu gerekçeyle
eklendi.

### 20.4 Asıl iş: drift kapısı

`tests/hint_translations.rs`, dört sessiz hatayı gürültülü yapıyor:

1. Kataloğa eklenip çevrilmemiş hint → o dilde İngilizce görünür.
2. Katalogdan silinip locale'de kalmış çeviri → kapsam gibi okunan ölü ağırlık.
3. `hints.rs` ile `en.js` arasında İngilizcenin ayrışması → fallback ile çeviri
   sessizce farklı şeyler söyler, ve hiçbir kullanıcı ikisini birden görmez.
4. Katalogda olup hiç kullanılmayan hint.

Dördü de mutasyonla denendi: bir Türkçe satır silindiğinde ve bir İngilizce
metin değiştirildiğinde build kırılıyor.

**Ve okuyucu iki kez kırıldı** — ikisi de kaydedilmeye değer, çünkü ikisi de
"test doğru şeyi kontrol ediyor ama yanlış sebeple düşüyor" sınıfı:

- İlk hâli satır bazlıydı. `prettier --write` uzun değerleri alt satıra taşıyor;
  okuyucu 12 anahtarı bulamayıp "çevrilmemiş" diye raporladı. Satır yerine
  **çifti** taramak biçimden bağımsız.
- Sonra bir anahtar daha kayboldu: Prettier, içinde apostrof olan değeri
  (`"…the project's Manifest tab…"`) **çift tırnağa** çeviriyor, çünkü daha az
  kaçış gerektiriyor. Tek tırnak bilen okuyucu tam olarak apostroflu satırları
  düşürüyordu.

### 20.5 Sayılar

|                      |   Önce |                             Sonra |
| -------------------- | -----: | --------------------------------: |
| Çevrilmeyen hint     |     60 | **3** (çalışma anında kurulanlar) |
| Türkçe hint çevirisi |      0 |                            **56** |
| Rust testleri        |    472 |                           **475** |
| Frontend testleri    |    182 |                           **186** |
| Rust kapsam          | %63.34 |                        **%63.47** |

Kullanıcının **eyleme geçtiği** metin artık Türkçe. Rapor bunu "çevrilmemesi en
pahalı olanı" diye işaretlemişti.

---

## 21. §14.14'ün ikinci yarısı — tanılama paketi

### 21.1 Neden bir zip, ve neye mal oldu

Rapor §4.2'de doğru teşhis koymuştu: Settings log **klasörünü** açıyor,
gerisini kullanıcıya bırakıyor. Yedi günlük dosyadan doğrusunu bulmak, doktor
çıktısının ayrı bir şey olduğunu bilmek, sürümü ve platformu hatırlamak. Çoğu
kişi en yeni logu ekliyor ve ilk yanıt hep diğer dört şeyin listesi oluyor.

`zip` bağımlılığı **eklemeden önce ölçüldü**: grafiğe tam olarak **bir** crate
ekliyor. `flate2` ve `miniz_oxide` zaten orada, ve `deflate-flate2` onları
yeniden kullanan tek sıkıştırma özelliği — varsayılan özellik kümesi yalnızca
metin taşıyan bir arşiv için aes, bzip2, zstd, xz, lzma ve ppmd getirirdi.
`zip`'in kendisi Windows'ta zaten derleniyor, çünkü `tauri-plugin-updater` onu
güncelleme açmak için alıyor.

### 21.2 İki kez maskeleme, ve neden batıl inanç değil

`logging::redact` alt süreç çıktısını yazarken zaten çalışıyor, yani diskteki
dosyalar maskeli. Her log satırı burada **ikinci kez** aynı kuraldan geçiyor.
Gerekçe: redaktör daha önce genişletildi, ve bugün toplanan bir paket, kuralı
daha dar olan bir sürümün yazdığı satırları içerebilir. Bugünkü kuralı eski
metne uygulamak birkaç megabaytlık bir geçiş maliyetinde, ve bir parolanın issue
tracker'a düşmesini engelleyen tek şey.

### 21.3 Kesilen şey söyleniyor

Log dosyası başına 1 MiB tavan. Eklenemeyen bir arşiv işe yaramaz, ve zaten bir
hatayı açıklayan kısım logun **sonu**. Ama kesilen şey `truncated` alanıyla ve
`README.txt` içinde **açıkça yazılıyor** — `applog::FanoutScan`'in kendi
tavanını raporlamasıyla aynı gerekçe: tam görünen kesik bir rapor, kısa olduğunu
söyleyen bir rapordan kötüdür.

### 21.4 Gönderilmeden önce okunabilir

Düz metin ve JSON, artı her dosyanın ne olduğunu anlatan bir `README.txt`.
Maskelemenin tüm önermesi paketin eklenmesinin güvenli olduğu — ama ekleyen
kişinin yine de bakabilmesi gerekir, ve açamadığı bir biçim kontrol edemediği
bir biçimdir. Arayüz de "kaydedildi" demiyor; **içindeki dosyaları adıyla**
listeliyor.

Uçtan uca doğrulandı: 9 dosya, 66 KB'lık içerik 9.9 KB'a sıkıştı, arşiv açıldı,
`README.txt` her girdiyi adlandırdı, ve loglarda maskelenmemiş tek bir
`PASSWORD=`/`TOKEN=`/`SECRET=` ataması yok.

### 21.5 Yan ürün: §2.1'in asıl riski kapandı

Yeni komut eklenince `readme_claims.rs` düştü — çünkü içinde sabit bir `143`
vardı. O sayı tarayıcı için bir akıl sağlığı kontrolüydü ve **her yeni komutu
tarayıcı hatası gibi gösteriyordu**: sinyal değil, gürültü.

Yerine gerçek bir değişmez kondu: `lib.rs`'in `generate_handler!` listesi ile
`commands.rs`'teki implementasyonlar **iki yönlü** karşılaştırılıyor.

Bu, `tools/validate-contracts.mjs` suite E'nin yaptığı işin yarısı — ama **o
job harici bir repo checkout'una bağlı**. Rapor bunu §2.1'de kusur değil _risk_
diye işaretlemişti ve haklıydı: `stackvo/stackvo` private olduğu, adı
değiştiği ya da rate-limit'e takıldığı gün sözleşme kapısı kaybolur ve kimse
fark etmez. Bu yarısı ağ, checkout ve Node istemiyor — o job koşamadığında da
koşuyor.

Yakaladığı hata sınıfı somut: `commands.rs`'e yazılıp `lib.rs`'e eklenmeyen bir
komut **derlenir ve sessizce geçer**; çalışma anında "command not found" olarak,
kimsenin geliştirme sırasında açmadığı bir ekranda ortaya çıkar.

### 21.6 Sayılar

|                         |   Önce |      Sonra |
| ----------------------- | -----: | ---------: |
| Rust testleri           |    475 |    **481** |
| `diagnostics.rs` kapsam |      — | **%94.58** |
| Rust toplam kapsam      | %63.47 | **%64.34** |
| IPC komutu              |    143 |    **144** |

§14'ün 9–15 grubundan geriye **§14.10 (tauri-specta)** ve **§14.12 (E2E)**
kaldı; ikisi de §18.4 ve §19.5'te gerekçeleriyle ayrı dal olarak işaretli.

---

## 22. §14.12 — E2E koşulamadı; boşluğa doğrulanabilir yoldan saldırıldı

### 22.1 Ölçüm: `tauri-driver` bu makinede çalışmıyor

Kurulup denendi. Derleniyor, sonra reddediyor:

```
$ tauri-driver --help
tauri-driver is not supported on this platform
```

macOS'ta WKWebView'ın WebDriver'ı yok. Yani §14.12'nin senaryoları **bu
makinede hiçbir şekilde koşturulamaz**; ancak bir Linux runner ilk kez
gördüğünde doğrulanabilirler. Koşulmamış test altyapısı göndermek, raporun
kendi tezinin ("_'muhtemelen aynı' shipping için bir standart değil_") tam
karşıtı olurdu. **§14.12 açık; bir Linux runner gerektiriyor.**

### 22.2 Ama boşluk E2E'nin kendisi değildi

§14.12'nin var olma sebebi `src/views/` **%0**'dı: kullanıcının baktığı şeyin
9.490 satırı, hiçbir testte tek satır çalışmıyor. Rapor bunu iki tanrı bileşene
yıkmıştı ve `Settings.vue` (3.433) ile `ProjectDetail.vue` (3.007) için haklı.
**Geri kalanı için değildi.** Yedi sayfa hep mount edilebilirdi ve sadece testi
yoktu — `Projects.vue` (1.022) ve `Mail.vue` (762) dahil, kimse denememişti.

### 22.3 Bulduğu şey: aynı hatanın dört örneği daha

§19.3'te bir tane bulunmuştu — `inventory.js`, sınırdan gelen cevabı doğrudan
atıyordu ve `null` geldiğinde pencere boşalıyordu. Sayfalar mount edilince
**aynı hatanın dört örneği daha** çıktı:

| Yer            | Komut                      |
| -------------- | -------------------------- |
| `LogView.vue`  | `app_logs_all`, `app_logs` |
| `DumpView.vue` | `debug_bridge_overview`    |
| `Projects.vue` | `project_adoptable`        |

Sözleşme taranınca liste dönen **yedi** atama daha korumasız çıktı
(`service_settings`, `container_stats_history`, `quick_commands`,
`templates_list`, `hosts_missing`, `hosts_missing_core`). Hepsi `asList` ile
kapatıldı — artık `src/lib/ipc.js`'te, sınırın kendi modülünde, tek yerde.

**Dördüncü kez aynı hatayı bulmak bir tesadüf değil, §2.2'nin ta kendisi.**
Sınır tipsiz olduğu sürece her yeni çağrı yeri aynı hatayı yeniden yazabilir.
`asList` bir yara bandı; `tauri-specta` ilacı.

### 22.4 Ve bir üretim hatası: `hintKey` düşüyordu

§14.11'de eklenen çeviri **üretimde hiç çalışmayacaktı.** `StackvoError`
constructor'ı payload'u destructure ediyor ve `hintKey`'i adlandırmıyordu — o
sınıf, gerçek bir hatanın `ErrorAlert`'e ulaştığı **tek** yol.

Testlerin yakalamamasının sebebi öğretici: hepsi düz nesne literal'i geçiyordu,
ve bir literal'in alanları testin yazdığı alanlardır. Suite yeşil kalırken
çeviri derlenmiş uygulamada hiçbir şey yapmayacaktı. Regresyon testi artık
sınıfa karşı, şekle karşı değil.

### 22.5 Axe, sayfalara açılınca dört bulgu daha

§19.5 "bu dosya, o listeye ekleme yapmak için bir sebeptir" diye bitiyordu. Yedi
sayfa eklendi ve dört gerçek ihlal çıktı:

- **`landmark-no-duplicate-banner`** — `PageLayout` **iki** `v-toolbar` üretiyor,
  ikisi de `<header>` → `banner`; `App.vue`'nunkiyle birlikte **üç**. Ekran
  okuyucu her sayfada üç kez "banner" duyuyor ve hiçbirini ayırt edemiyordu.
  İkisi de `tag="div"` yapıldı — sayfa içi bir çubuk pencerenin banner'ı değil.
- **`label`** — `LogView` ve `DumpView`'ın proje seçicileri yalnızca
  `placeholder` taşıyordu. Placeholder erişilebilir ad değildir: yazılır
  yazılmaz kaybolur, ve kontrol "adsız combobox" diye okunur.
- **`aria-progressbar-name`** — `Dashboard`'ın üç yükleme dönerinde ve
  `Mail`'in dördünde ad yok. `StatCard` ile aynı sınıf (§19.5).
- **`empty-table-header`** — Vuetify'ın `VDataTableHeaders.js:226` satırı
  koşulsuz bir `<th colspan="{n+1}">` yükleme satırı üretiyor. Gerçek bir bulgu
  ama **burada yazılmamış** ve hiçbir prop/slot ile kontrol edilemiyor. Yalnızca
  _sayfa_ taramalarında, kaynağı adlandırılarak kapatıldı; bileşen taramalarında
  kural açık kalıyor — `aria-progressbar-name`'i her yerde kapatmak, bu dosyayı
  haklı çıkaran bulguyu çöpe atmak olurdu.

### 22.6 Sayılar

|                            |   Önce |      Sonra |
| -------------------------- | -----: | ---------: |
| **Frontend toplam kapsam** | %31.44 | **%50.71** |
| **`src/views/`**           | **%0** | **%26.38** |
| `About` / `Dumps` / `Logs` |     %0 |   **%100** |
| `Services`                 |     %0 | **%94.38** |
| `Dashboard`                |     %0 | **%85.42** |
| `Projects`                 |     %0 | **%84.25** |
| `Mail`                     |     %0 | **%78.78** |
| Frontend testleri          |    186 |    **228** |
| Rust testleri              |    481 |        481 |

Geriye `Settings.vue` ve `ProjectDetail.vue` kaldı — ikisi de **%0**, ve ikisi de
§14.16'nın (bölme) konusu. `src/views/`'in %26'da kalmasının tek sebebi onlar.

### 22.7 Açık kalanlar

- **§14.12 E2E** — bir Linux runner gerekiyor. Klavye-only gezinme, focus
  tuzakları ve `color-contrast` (§19.5) hâlâ yalnızca orada ölçülebilir.
- **§14.10 tauri-specta** — §22.3'ün dördüncü kez bulduğu hatanın tek kalıcı
  çözümü.
- **§14.16** — `Settings.vue` ve `ProjectDetail.vue`.

---

## 23. §14.16 — tanrı bileşeni bölmek, ilk iki dilim

### 23.1 Neden bu iki panel

`Settings.vue` 3.433 satır ve **%0** kapsamdı. Rapor §2.3'te bunun en pahalı
sonucunu doğru tespit etmişti: iki test — `certificates-pane.spec.js` ve
`template-overrides.spec.js` — paneli **mount etmiyor**, markup'ın bir
_kopyasını_ test dosyasında yeniden kuruyor, sonra gerçek dosyayı metin olarak
okuyup kopyanın hâlâ eşleştiğini doğruluyordu.

Bu ikisiyle başlandı çünkü ikisi de **gerçekten kırık gönderilmiş** bir davranış
için yazılmıştı:

- **Sertifikalar:** `v-tooltip`, `v-icon`'un içine ikonun kendi adıyla birlikte
  yerleştirilmişti; slot iki şey tutuyordu ve hover hiçbirine ulaşmıyordu.
  Hiçbir şey yakalamadı, çünkü hiçbir şeye render olan markup temiz lint'lenir
  ve temiz derlenir.
- **Şablonlar:** düğme **dönerek** gönderildi. Bağlama
  `templateBusy === templateToOverride` idi — doğru okunur, ve panelin
  açıldığı durumda yanlıştır: ikisi de `null` başlar, `null === null`, ve düğme
  kimse dosya seçmeden önce kendini meşgul ilan eder — ve her başarılı
  devralmadan sonra tekrar, çünkü seçim `null`'a döner.

Yani kopya-testler **doğru soruyu soruyordu**; sorun cevabı üründe değil
kopyada aramalarıydı.

### 23.2 Ne yapıldı

|                                                 |                                                                                                                                                                             |
| ----------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `composables/useCertificates.js`                | Durum + `load` / `reissue` / `trustInTerminal`. Modül kapsamlı, çünkü panel **ve** ayar rayının "sertifika bayat" rozeti aynı veriyi okur ve aynı cevabı vermek zorundadır. |
| `composables/useTemplates.js`                   | Çağrı başına kendi durumu — tek tüketici var. `busyWith` burada, düzeltmesiyle birlikte.                                                                                    |
| `components/settings/CertificatesPane.vue`      | 242 satır, **%100 kapsam**                                                                                                                                                  |
| `components/settings/TemplateOverridesPane.vue` | 158 satır, **%100 kapsam**                                                                                                                                                  |

Kopya-testlerin ikisi de silindi; yerlerine gerçek bileşeni mount eden
`settings-certificates.spec.js` (8 test) ve `settings-templates.spec.js`
(9 test) geldi.

### 23.3 Yeni testler eskisinin ölçemediğini ölçüyor

Kopya-testler yalnızca **bir** davranışa bakabiliyordu: hover, ve dönen düğme.
Panelin geri kalanı erişilemezdi. Gerçek bileşen mount edilince aynı dosyalar
şunları da kapsıyor:

- SSL kapalıyken panelin hiçbir şeyin geçerli olmadığını söylemesi
- mkcert yokken yeniden üretme düğmesinin **disabled** olması
- CA güvenilirken terminal düğmesinin _görünmemesi_
- Yeniden üretme başarılı ama proxy eskisini servis ediyorken uyarı
- Workspace yokken sessiz kalıp başka her hatayı göstermesi
- Şablon listesinde devralınanların ayrılması, sınır bozuk cevap verdiğinde
  fırlatmak yerine boş liste
- Devralmanın dosyayı kopyalayıp editörde açması ve seçimi temizlemesi
- Geri almanın **diyalog onaylanmadan** çalışmaması

`busyWith` kılavuzu mutasyonla denendi: bozuk hâline döndürüldüğünde iki test
birden düşüyor — hem "seçim yokken dönüyor" hem "bitince tekrar dönüyor".

### 23.4 Ve iki test kalıbı düzeltildi

- **Kaynak metnine yapılan iddialar gitti.** Eski test
  `const busyWith = (path) => !!path && templateBusy.value === path` satırının
  birebir varlığını iddia ediyordu. Çalışıyordu ve bir string'e çakılıydı:
  kılavuz taşınsa, yeniden adlandırılsa ya da sarmalansa test düğmeyle hiç
  ilgisi olmayan bir sebeple düşerdi.
- **İlk yazdığım hâli de yanlıştı.** Dönme testini `wrapper.vm.working = …`
  diye iç duruma dokunarak yazmıştım — kopyanın yaptığı hatanın aynısı, şekle
  bakmak. Gerçek akışa çevrildi: sınır açık tutuluyor, düğmeye tıklanıyor,
  spinner DOM'dan okunuyor, sonra sınır serbest bırakılıp **idle'a dönüş**
  doğrulanıyor — ki asıl hata oydu.

### 23.5 Sayılar

|                                              |        Önce |           Sonra |
| -------------------------------------------- | ----------: | --------------: |
| `Settings.vue`                               | 3.433 satır |       **2.938** |
| Shape-mirror testi                           |           2 |           **0** |
| `CertificatesPane` / `TemplateOverridesPane` |           — | **%100 / %100** |
| `src/composables`                            |           — |      **%95.31** |
| Frontend toplam kapsam                       |      %50.71 |      **%53.74** |
| Frontend testleri                            |         228 |         **241** |

Kalan dokuz panel ve `ProjectDetail.vue` aynı kalıpla devam eder. `Settings.vue`
hâlâ %0 — onu mount etmek bütün panellerin çıkmasını bekliyor.

---

## 24. §14.16 devam — üçüncü dilim, ve onun bulduğu i18n hatası

### 24.1 Yalnızca yarısı çıkarılabildi, ve bu bir bulgu

`servers` sekmesi iki gruptan oluşuyor. İkincisi — sunucu başına ek direktif
dosyası — kendi kendine yetiyor ve çıkarıldı. **Birincisi çıkarılamadı:** limit
formu, altı panelin paylaştığı `.env` düzenleme makinesini (`dirty`, `saving`,
`edits`, `effective`, `edit`, `onOff`) sürüyor.

Bu, raporun §2.3'te adını koyduğu `useEnvEditor()`'ün ta kendisi ve ayrı bir iş.
Kaydedilmeye değer, çünkü bölmenin kalan maliyetinin nerede toplandığını
gösteriyor: kalan sekizden dördü aynı makineye bağlı, yani sıradaki tek büyük
adım o makineyi çıkarmak.

Çıkarılan: `useServerConfig` + `ServerDirectivesPane.vue`, **%100 kapsam**,
6 test.

### 24.2 Test, markup'ta izi olmayan davranışa bakıyor

Bu panelin ilk ikisinden farkı, arkasında bir shape-mirror **olmaması** — o
ikisi biri hata yaşadığı için vardı. Buranın ne testi ne hatası vardı, ama en
kırılgan davranışı gözle görülmüyor: **sekme değişince dosyanın yeniden
yüklenmesi.** Unutan bir sürüm nginx'in direktiflerini caddy sekmesinde
gösterir ve sonra **oraya kaydeder** — yanlış sunucunun config'ini doğrusunun
içeriğiyle sessizce ezer. Markup'ta bunun olabileceğini ima eden hiçbir şey yok.

### 24.3 Bu dilimin asıl getirisi: 4 bozuk çeviri dizesi

Paneli mount edince vitest her render'da şunu bastı:

```
Message compilation error: Not allowed nest placeholder
1  |  {{ VAR }} is substituted from .env. …
```

vue-i18n `{…}`'i placeholder okur, yani `{{ VAR }}` **iç içe** bir
placeholder'dır ve yasaktır. Gürültülü değil, _sessiz_: derleyici hatayı
loglar, ham dizeye düşer, metin doğru görünür ve her render konsola hata yazar.

Bunu bir teste bağladım — `tests/i18n.spec.js` artık **her mesajı** iki dilde
derletiyor — ve kapı açılır açılmaz **üç tane daha** çıktı:

| Anahtar                        | Sorun                                          |
| ------------------------------ | ---------------------------------------------- |
| `settings.servers.extraHint`   | `{{ VAR }}` — iç içe placeholder               |
| `mail.searchPlaceholder`       | `from:a@b.c` — çıplak `@` bağlı-mesaj başlatır |
| `newProject.gitUrlPlaceholder` | `git@server…` — aynısı                         |

Dördü de iki dilde, yani **8 dize**. Hepsi vue-i18n'in literal kaçışıyla
(`{'@'}`, `{'{{ VAR }}'}`) düzeltildi ve render edilen metnin **birebir aynı**
kaldığı doğrulandı.

Bu, §11'in tezinin bir örneği daha: yanlış olduğunda hiçbir şeyin şikâyet
etmediği bir yüzey, yanlış olduğunda hiçbir şeyin şikâyet etmediği için yanlış
kalır. Panelin mount edilmesi tek bir tanesini görünür yaptı; **kapı geri
kalanını buldu.**

_(Testin ilk hâli de yanlıştı: `flatten` dizileri de düzleştirdiği için
`nav.items.0` gibi sahte anahtarlar üretip vue-i18n'e "böyle bir anahtar yok"
uyarısı bastırıyordu — kapının kendi gürültüsü, bulgu diye okunabilirdi. Yalnızca
string yapraklara bakacak şekilde daraltıldı.)_

### 24.4 Ve iki sahipsiz stil

`.why-separate` (sertifika ikonunun `cursor: help`'i) ve `.server-config`
(monospace textarea) `Settings.vue`'nun scoped bloğunda kalmıştı. **Scoped stil
bir elemanı başka bir bileşene takip etmez**, yani ilk çıkarma sessizce imlecin
değişmesine yol açmıştı — hiçbir testin ve lint'in göremeyeceği bir gerileme.
İkisi de bileşenlerine taşındı.

### 24.5 Sayılar

|                   | §23 sonrası |      Şimdi |
| ----------------- | ----------: | ---------: |
| `Settings.vue`    | 2.938 satır |  **2.848** |
| Çıkarılan panel   |           2 |      **3** |
| Bozuk i18n dizesi |           8 |      **0** |
| Frontend kapsam   |      %53.74 | **%54.37** |
| Frontend testleri |         241 |    **248** |

Kalan sekiz panelin dördü paylaşılan `.env` editörüne bağlı — sıradaki adım o.

---

## 25. §14.16 — `useEnvEditor`, bölmenin kilidi

### 25.1 Neden bu, sıradaki panel değil

§24.1'de kaydedilmişti: kalan sekiz panelin dördü aynı `.env` düzenleme
makinesine bağlı — dört ref (`env`, `defaults`, `edits`, `saving`) ve on
yardımcı üzerinden **aynı dosyayı** yazıyorlar. Yani hiçbiri tek başına
`Settings.vue`'dan çıkamıyordu. Bir sonraki paneli çıkarmak yerine kilidi
açmak, kalan işin şeklini değiştiriyor.

`useEnvEditor` çıkarıldı, **%100 kapsam**, 17 test. `Settings.vue` yalnızca
32 satır kısaldı — ama bu sayı yanıltıcı: çıkan şey satır değil, **bağımlılık**.

### 25.2 Test edilmemiş olması en dikkat çekici kısmıydı

Bu makinenin hiç testi yoktu. Altı panel stack'in yapılandırma dosyasını
üzerinden yazıyor, ve _ne yazılacağına_ karar veren parçalar üç satırlık ok
fonksiyonları — yani hata verene kadar hiç hata vermeyen, verdiğinde de aynı
anda her yerde veren kod sınıfı.

Üçü mutasyonla denendi, üçü de yakalanıyor:

| Karar                                                  | Yanlış olduğunda                                                                                                                                                                                                    |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Üç katmanlı okuma (`edits → env → defaults`)           | Form "bu varsayılan" diyemez; her değer eşit derecede seçilmiş görünür.                                                                                                                                             |
| `edit()`'in değer geri geldiğinde anahtarı **silmesi** | Bir karakter yazıp geri silmek diff'te iz bırakır; kaydet düğmesi yanar, kayıt diskteki değerin aynısını yazar, ve yönlendirme anahtarıysa **kimsenin yapmadığı bir değişiklik için** "yeniden üret" uyarısı çıkar. |
| İki boolean yazımının ayrı tutulması                   | `.env`'de compose `true`/`false`, üretilen nginx ve php.ini parçaları `on`/`off` okur. Yanlışını yazmak, **parse edilen ve anahtarın söylediğinin tersini yapan** bir dosya üretir.                                 |

Sonuncusu bu kod tabanının en sessiz hata sınıfı: dosya geçerli, uygulama
çalışıyor, ve ayar tam tersini yapıyor.

### 25.3 Bir davranış açıkça sözleşmeye bağlandı

Eski `save()` içinde `await app.refreshTld()` gömülüydü. Composable'a taşınırken
bu bir **geri çağırma** oldu (`save(onSaved)`) ve sırası teste bağlandı: mağazanın
önbelleklediği TLD, "kaydedildi" onayı ekrana gelmeden **önce** güncellenmeli —
yoksa uygulamanın gösterdiği her alan adı, bir yeniden yüklemeye kadar önceki
son ekte kalır.

### 25.4 Sayılar

|                        | §24 sonrası |      Şimdi |
| ---------------------- | ----------: | ---------: |
| `Settings.vue`         | 2.848 satır |  **2.816** |
| Composable             |           3 |      **4** |
| `useEnvEditor` kapsamı |           — |   **%100** |
| Frontend kapsam        |      %54.37 | **%54.96** |
| Frontend testleri      |         248 |    **265** |

Kilit açıldı: kalan sekiz panel artık teker teker çıkarılabilir, çünkü hepsi
aynı composable'ı çağırıp kendi markup'ını taşıyabilir. `Settings.vue` hâlâ %0 —
son panel çıkana kadar öyle kalacak.

---

## 26. §14.16 devam — `useStackShape`, ve bölmenin gerçek riski

### 26.1 Çıkarılan

`domain` panelinin arkasındaki üç mantık kümesi `useStackShape.js`'e taşındı,
**%100 kapsam**, 40 test:

|                    |                                                                                   |
| ------------------ | --------------------------------------------------------------------------------- |
| `useStackShape`    | Son ekin iki yarıya bölünmesi, dört doğrulama kuralı, HSTS uyarısı, kaydet kapısı |
| `useHostsOverview` | Eksik ve bayat girdiler, tek yükseltme isteminde iki yönlü düzeltme               |
| `useProxy`         | Traefik'in durumu ve kendi panosunun adresi                                       |

Doğrulama süs değil: `DEFAULT_TLD_SUFFIX` doğrudan `Host(\`shop.SUFFIX\`)`
içine giriyor ve **aşağıda hiçbir yer onu bir daha kontrol etmiyor**. Boşluklu
bir son ek, parse edilen bir compose dosyası, ayağa kalkan bir stack ve çözülmeyen
tek bir adres üretir. Bu kurallar 2.816 satırlık bir bileşenin içinde, testsiz
duruyordu — kimse test etmek istemediği için değil, onlara ulaşmanın görünümü
mount etmek anlamına geldiği için.

### 26.2 Ve bu turun asıl bulgusu: ben kırdım, hiçbir şey görmedi

Çıkarma sırasında yazdığım silme betiği, niyetlenilen bloğun **çok ötesine**
geçip `Settings.vue`'dan üç sabiti daha aldı: `SERVER_SUPPORT`, `sizeRules`,
`secondsRules` — ve `RUNTIME_DEFAULTS`. Şablon dördünü de kullanmaya devam
ediyordu.

Yani: **uygulama render'da patlayacaktı**, ve

- `eslint` sustu — şablon tanımlayıcılarını script binding'leriyle karşılaştırmıyordu,
- `prettier` sustu,
- 265 testin **hiçbiri** düşmedi, çünkü ilgili paneller henüz mount edilmiyor,
- `vue/valid-*` kuralları sözdizimine bakar, varlığa değil.

Yakalayan tek şey **§24.3'te eklenen i18n kapısı** oldu: silinen kurallar iki
çeviri anahtarını erişilemez bıraktı ve "kullanılmayan çeviri" testi düştü. Bir
gate'in kendi konusu dışında bir hatayı yakalaması tesadüf; **tesadüfe
güvenilmez.**

**Alınan önlem:** `vue/no-undef-properties` açıldı. Sabiti tekrar silerek
denendi:

```
1860:34  error  'SERVER_SUPPORT' is not defined  vue/no-undef-properties
```

Bu, §14.16'nın kalan sekiz paneli için asıl risk azaltımı. Bölme, tanımı bir
dosyadan alıp diğerine koymaktır ve bu sınıf hatanın tek görünür olduğu katman
buydu — çünkü panellerin çoğu hâlâ mount edilmiyor.

_(Dosya `git checkout` ile geri alınıp düzenlemeler kesin sınırlarla yeniden
uygulandı. Yamalayarak ilerlemek, ne kadarının gittiğini bilmeden tahmin etmek
olurdu.)_

### 26.3 Sayılar

|                         | §25 sonrası |      Şimdi |
| ----------------------- | ----------: | ---------: |
| `Settings.vue`          | 2.816 satır |  **2.746** |
| Composable              |           4 |      **7** |
| `useStackShape` kapsamı |           — |   **%100** |
| Frontend kapsam         |      %54.96 | **%55.66** |
| Frontend testleri       |         265 |    **305** |

`domain` panelinin **markup'ı** hâlâ `Settings.vue`'da — bu tur mantığı çıkardı,
bileşen bir sonraki adım. Kalıp (§33.2) değişmedi; yalnızca 1. adımı bitirdi.

---

## 27. §14.16 — `DomainPane`, ve paylaşılan durumun taşınması

### 27.1 Dördüncü panel

`domain`'in markup'ı da çıktı: `DomainPane.vue`, **%100 kapsam**, 12 test.
`Settings.vue` **2.746 → 2.398**.

Bu, paylaşılan `.env` editörüne ihtiyaç duyan **ilk** panel. Prop olarak
geçirmek işe yarardı ve hiçbir panelin _seçmediği_ bir değeri her panel
imzasından geçirmek anlamına gelirdi; `provide`/`inject` tam olarak bunun
içindir. `useSharedEnvEditor()`, kimse sağlamamışsa kendi örneğini kuruyor —
ve bu, paylaşılan duruma bağlı bir panelin **tek başına mount edilebilmesinin**
tek sebebi.

Panel `.env`'i kendisi **okumuyor**, bilerek: altı panel tek dosya üzerinde tek
bir diff paylaşıyor, ve her mount'unda yükleyen bir panel kullanıcı sekme
değiştirdiğinde diğerlerinin yazdığını sessizce atardı.

### 27.2 Testin ilk hâli bunu bilmiyordu ve dört kez düştü

Paneli çıplak mount ettim. Dört iddia düştü — önizleme boş, kaydet düğmesi
kapalı, HSTS uyarısı sessiz — ve dördü de **doğru davranıştı**: yüklenmemiş bir
editörle her alan boş, dolayısıyla kaydet düğmesinin kapalı olması gerekir.
Yani dört kırık iddia gibi görünen şey, tek bir eksik `load()`'du.

Test, uygulamanın yaptığını yapan bir host bileşenin altına alındı: editörü
sağlayan ve **yükleyen** bir ebeveyn. Kaydetmesi gereken ders şu: bir bileşeni
gerçekte içinde yaşamadığı bir bağlamda mount etmek, testi yeşil yapmaz —
yanlış soruyu sormasını sağlar.

### 27.3 Ve bir öncülüm yanlıştı

"Boş TLD kaydedilebilir olmamalı" diye bir test yazdım. Yanlış: TLD yarısını
temizlemek `stackvo` bırakır — tek etiketli bir suffix, ki **meşru** (`loc` tek
başına da öyle). Kural setini okumadan davranışı tahmin etmiştim. Test gerçekten
reddedilen bir değere (`lo c`) çevrildi.

### 27.4 Yeni lint kuralı ikinci kez işe yaradı

`vue/no-undef-properties` (§26.2'de eklendi) bu turda iki eksik binding yakaladı:
`isDefault` ve `stackBusy`, markup taşındığında geride kalmışlardı. İkisi de
render'da patlardı, hiçbir test görmezdi — çünkü panel o an henüz mount
edilmiyordu.

### 27.5 Sayılar

|                      | §26 sonrası |      Şimdi |
| -------------------- | ----------: | ---------: |
| `Settings.vue`       | 2.746 satır |  **2.398** |
| Çıkarılan panel      |           3 |      **4** |
| `DomainPane` kapsamı |           — |   **%100** |
| Frontend kapsam      |      %55.66 | **%57.66** |
| Frontend testleri    |         305 |    **318** |

Başlangıçtaki 3.433 satırdan **%30 düşüş**. Kalan yedi panel aynı kalıpla
devam eder; `provide`/`inject` kurulduğu için `.env`'e bağlı olanlar artık
teker teker çıkarılabilir.

---

## 28. §14.16 — `WorkspacePane`, ve hiç çalışmayan bir yükleme

### 28.1 En büyük panel

`workspace` çıktı: `WorkspacePane.vue` (%98.74) + `useStackPreset` /
`useGeneratorCheck` (%94.67), 16 test. `Settings.vue` **2.398 → 1.959** —
başlangıçtaki 3.433'ten **%43 düşüş**.

Compose fiilleri (`up` / `restart` / `down`) ve klasör seçici **emit ediliyor**,
çağrılmıyor: paylaşılan işlem konsoluna raporlanıyorlar ve meşguliyet durumu
görünümün. Kendi çalıştıran bir panel, stack'e sahip olan bir panel olurdu.

### 28.2 Ve bulduğu hata: dışa aktarma kartı hiç dolmuyordu

`Settings.vue` preset'i etkin sekmeyi izleyen bir `watch`'tan yüklüyordu:

```js
if (value === 'sharing' && !stackPreset.value) loadStackPreset();
```

**`sharing` diye bir bölüm yok.** Klasör, compose fiilleri ve preset tek bir
`workspace` paneline birleştirilmiş, anahtar geride kalmıştı. `loadStackPreset`'e
giden hayatta kalan tek yol, bir içe aktarma _başarılı olduktan sonraki_
çağrıydı — yani paneli açan kullanıcı boş bir JSON kutusu ve "0 servis açık"
görüyordu.

Hiçbir şey yanlış görünmüyordu. Var olmayan bir string ile karşılaştırma yapan
bir `watch`, hiçbir aracın raporladığı bir hata değil; ve paneli mount eden bir
test olmadığı için kimse fark etmedi. Panel artık mount'ta yüklüyor — her bölüm
bir `v-if` arkasında olduğu için **mount etmek zaten açmaktır**, ve ayak
uydurulacak bir anahtar kalmıyor.

`it('loads the current stack as soon as it opens')` bu hatanın geri gelirse
düşen hâli.

### 28.3 Testimin üç kurgu hatası

Bu tur test kurgusu üç kez yanlıştı ve üçü de kaydedilmeye değer, çünkü üçü de
"test yeşil olsun diye" değil "test doğru soruyu sorsun diye" düzeltildi:

1. **Yanlış i18n anahtarları.** Butonları `settings.stack.up` ile aradım;
   gerçekte `actions.up`. Buton bulunamadı, test "buton yok" dedi — doğru
   şikâyet, yanlış sebep.
2. **Motor kapalıydı.** Compose butonları `!app.engineUp` ile devre dışı; taze
   bir store motoru kapalı bildirir. Bu **doğru davranış**, ve bu butonların
   konusu olan dünya değil. Ayrı bir test olarak da sabitlendi.
3. **İki ayrı Pinia.** `mount`'a verilen pinia ile testin `useAppStore()`
   çağrısının çözdüğü pinia farklıydı: `engineUp` testte `true`, panelde
   `false` okuyordu ve butonlar devre dışı kalıyordu. `useAppStore(pinia)` ile
   açıkça verildi.

Üçüncüsü en sinsisi: iki store örneği olan bir test, yazdığı durumun test
ettiği şeye görünmediği bir testtir — ve bunu söyleyen tek şey, iddianın
sebepsiz düşmesiydi.

### 28.4 Yeniden adlandırma i18n anahtarlarına sızdı

`stackPreset` → `preset` yeniden adlandırmam `t('stackPreset.export')`
çağrılarının içine de girdi ve 21 anahtarı `t('preset.export')` yaptı. §24.3'te
eklenen i18n kapısı ikisini birden yakaladı: "uygulamanın istediği anahtarlar"
ve "kullanılmayan çeviriler" testleri aynı anda düştü.

### 28.5 Sayılar

|                   | §27 sonrası |      Şimdi |
| ----------------- | ----------: | ---------: |
| `Settings.vue`    | 2.398 satır |  **1.959** |
| Çıkarılan panel   |           4 |      **5** |
| Composable        |           7 |      **9** |
| Frontend kapsam   |      %57.66 | **%60.26** |
| Frontend testleri |         318 |    **335** |

`Settings.vue` başlangıçtaki 3.433 satırdan **%43** küçüldü. Kalan altı panel:
`appearance` (~276), `php` (~163), `servers` limitleri (~155), `preferences`
(~94), `doctor` (~94), `services` (~79), `localisation` (~55).

---

## 29. §14.16 — `AppearancePane`, ve axe'ın iki kaydırıcısı

### 29.1 En temiz dikiş

`appearance` çıktı: `AppearancePane.vue`, **%100 kapsam**, 6 test.
`Settings.vue` **1.959 → 1.657** — başlangıçtaki 3.433'ten **%52 düşüş**.

Şimdiye kadarki en temiz ayrım: ne `.env` editörüne ne işlem konsoluna
dokunuyor. Değiştirdiği her şey `useAppearanceStore`'da yaşıyor ve store kendi
kendine kalıcılaştırıp uyguluyor — yani panel, bir store'un üzerindeki
markup'tan ibaret.

### 29.2 Test neyi ölçüyor

Store'un kendi kapsamı var, dolayısıyla burada değerli olan yalnızca markup
çalışınca var olan üç şey:

- **Sıfırlama göstergesi doğru söylüyor mu** — bayrak store'u takip etmeli;
  bayat bir bayrak ya hiçbir şey yapmayan bir sıfırlama sunar ya da işe
  yarayacak olanı gizler.
- **İsimsiz hazır ayar kaydedilemiyor**, ve kaydetme alanı temizliyor — aksi
  halde bir sonraki hazır ayara öncekinin adı öneriliyor ve ikinci tıklama onu
  sessizce eziyor.
- **Kütüphanenin gönderdiği her palet, font ve renk gerçekten sunuluyor mu** —
  elle yazılmış bir alt küme, `appearance.js`'e eklenen bir paletin uygulamada
  hiç görünmemesinin ve kimsenin bunu raporlamamasının yoludur.

### 29.3 Ve iki erişilebilirlik ihlali daha

Panel mount edilir edilmez axe iki kaydırıcıyı bildirdi: yarıçap ve arayüz
ölçeği. İkisinin de **görünür bir etiketi var** — üstlerinde bir
`<div class="field-label">` — ama programatik bir bağı yok. Ekran okuyucu
"kaydırıcı, 12" diyor ve neyin 12'si olduğunu söylemiyor. İkisine de
`aria-label` eklendi.

Kalan bir ihlal Vuetify'ın kendisinden: `v-slider`, gerçek `role="slider"`
kontrolünün yanında yalnızca form değeri taşısın diye gizli bir
`<input tabindex="-1">` üretiyor. Etiketi yok ve verilemiyor — Vuetify ona
hiçbir öznitelik geçirmiyor — ve odaklanılamadığı için hiçbir kullanıcı onunla
karşılaşmıyor. `label` kuralı **yalnızca bu panel için** kapatıldı, her yerde
değil: o kural gerçek etiketsiz alanları yakalayan kural ve §22.5'te
`LogView` ile `DumpView`'da tam olarak onu yakalamıştı.

### 29.4 Sayılar

|                   | §28 sonrası |      Şimdi |
| ----------------- | ----------: | ---------: |
| `Settings.vue`    | 1.959 satır |  **1.657** |
| Çıkarılan panel   |           5 |      **6** |
| Frontend kapsam   |      %60.26 | **%61.93** |
| Frontend testleri |         335 |    **342** |

Çıkarılan altı panelin altısı da **%98–100** kapsamda. Kalan beş panel:
`php` (~163), `servers` limitleri (~155), `preferences` (~94), `doctor` (~94),
`services` (~79), `localisation` (~55).

---

## 30. §14.16 — `PhpPane` ve `LocalisationPane`

### 30.1 İki panel daha

`php` ve `localisation` çıktı, ikisi de **%100 kapsam**, 10 test.
`Settings.vue` **1.657 → 1.415** — başlangıçtaki 3.433'ten **%59 düşüş**.

`php`, katalog seçimlerini de beraberinde getirdi: `useCatalog` modül kapsamlı,
çünkü iki panel (`php` ve `servers`) aynı katalogu okuyor ve tek bir istekten
aynı cevabı almaları gerekiyor.

### 30.2 Pinlenmeye değer tek kural

`itemsFor`: **`.env`'de yazılı olan değer her zaman listede.** Katalog onu
bilmese bile.

Buraya iki sıradan yoldan gelinir — başarısız bir katalog çağrısı, ve daha eski
bir sürümün yazdığı bir değer. İkisinde de tek öğesi eksik olan bir select boş
render eder, ki bu "artık bu sürüm gönderilmiyor" değil **veri kaybı** gibi
okunur. Dört test bunu sabitliyor: katalogtan gelen liste, listede olmayan
yazılı değer, okunamayan katalog, ve ikisinin de bilmediği durum.

### 30.3 `localisation`: üç kontrol, üç ayrı sahip

En küçük panel ve tam da bu yüzden mount edilmeye değer. Uygulama dili
`setLocale`'den geçiyor — çünkü tercihi de kalıcılaştırıyor ve tepsiyi yeniden
etiketliyor. Konsol dili ve RTL bayrağı appearance durumu ve doğrudan store'a
gidiyor.

Üçünden birini yanlış sahibe bağlamak, **çalışıyor görünen ve bir sonraki
açılışta kendini unutan** bir kontrol üretir. Test üçünü de sahibine karşı
doğruluyor, ve appearance ayarlarının `setLocale`'i _çağırmadığını_ da.

### 30.4 Sayılar

|                   | §29 sonrası |      Şimdi |
| ----------------- | ----------: | ---------: |
| `Settings.vue`    | 1.657 satır |  **1.415** |
| Çıkarılan panel   |           6 |      **8** |
| Composable        |           9 |     **10** |
| Frontend kapsam   |      %61.93 | **%63.55** |
| Frontend testleri |         342 |    **354** |

Çıkarılan sekiz panelin sekizi de **%98–100** kapsamda. Kalan dört panel:
`servers` limitleri (~155), `preferences` (~94), `doctor` (~94),
`services` (~79).

---

## 31. §14.16 — `Settings.vue` bitti

### 31.1 Bitiş çizgisi

Son dört panel çıktı (`servers` limitleri, `services`, `preferences`,
`doctor`) ve **`Settings.vue` artık mount edilebiliyor.**

|                        |   Başlangıç |      Şimdi |
| ---------------------- | ----------: | ---------: |
| `Settings.vue`         | 3.433 satır |    **831** |
| `Settings.vue` kapsamı |      **%0** | **%90.97** |
| Çıkarılan panel        |           0 |     **12** |
| Composable             |           0 |     **11** |
| `src/views/` kapsamı   |          %0 |  **%47.2** |
| Frontend toplam kapsam |      %30.70 | **%73.85** |
| Frontend testleri      |         160 |    **372** |

Görünüm artık hiç panel markup'ı tutmuyor — yalnızca ray, paylaşılan `.env`
editörü ve Hakkında kartı. Çıkarılan on iki panelin onu **%97–100** kapsamda.

### 31.2 Asıl kazanç: sekme değiştirmek artık test ediliyor

`tests/settings-view.spec.js` görünümü mount edip **on bir bölümün hepsini**
tek tek açıyor, iki dilde.

Bu, bölmeden önce yapılamayan kontrol. Her panel bir `v-if` arkasında,
dolayısıyla görünümün sağlamayı bıraktığı bir şeye atıf yapan bir panel
**yalnızca kendi sekmesi seçildiğinde** patlar — yani onu bozan değişiklik
sırasında kimsenin açmadığı bir ekranda. Bölme boyunca `vue/no-undef-properties`
bunun üç gerçek örneğini yakaladı (§26.2, §27.4); artık kalanını bu test
yakalıyor.

Bir test de paylaşılan diff'i doğruluyor: `domain` panelinde yapılan bir
düzenleme, `php` paneline geçildiğinde görünüyor. İki panel tek dosya üzerinde
iki ayrı diff tutsaydı, en son kaydeden diğerinin işini sessizce atardı.

### 31.3 Bu turun bulguları

- **`ServerDirectivesPane`'in metin kutusunun adı yoktu** — yalnızca placeholder
  taşıyordu, ki placeholder yazılır yazılmaz kaybolur ve erişilebilir ad
  değildir. §22.5'teki iki select ile aynı sınıf; axe onu ancak panel bir
  ebeveynin içinde tarandığında gördü.
- **`renderPane` çıplak mount ediyordu.** `ServicesPane` bir navigation drawer
  taşıyor ve Vuetify'ın layout injection'ı `v-app` istiyor. Aynı dersi
  `views-render.spec.js` §22'de öğrenmişti; axe yardımcısına da uygulandı.
- **Bayat dilim dosyaları.** `servers` çıkarıldıktan sonra satır numaraları
  kaydı ve önceden alınmış dilimler yanlış içerik taşıdı — bir panel bileşeni
  başka bir panelin markup'ıyla üretildi. Yeniden dilimlenerek düzeltildi;
  §30.2'nin "kesme sınırını doğrula" kuralının ikinci hâli: **kestiğin dosyayı
  değiştirdikten sonra dilimlerini yeniden al.**

### 31.4 Kalan

`ProjectDetail.vue` (3.007 satır, **%0**) — aynı kalıp, aynı bitiş çizgisi.
`src/views/`'i %47'de tutan tek şey artık o.

---

## 32. §14.16 — `ProjectDetail.vue` başladı

### 32.1 İlk panel

`ProjectDetail.vue` (3.007 satır, **%0**) bölünmeye başladı. İlk çıkan
`indicator`: `useContainerStats` (**%100**, 24 test) ve `IndicatorPane.vue`
(**%100**, 4 test). Görünüm **3.007 → 2.741**.

`Settings.vue`'dan farklı bir yapı: yedi bölümün blokları **iç içe geçmiş** —
`debug` üç ayrı yerde, `container` üç, `configuration` üç. Bir bölümün tüm
blokları aynı `v-if` altında sırayla render edildiği için tek bileşende
toplanmaları görsel sırayı koruyor; harita §32.4'te.

### 32.2 Timer görünümde kalıyor, panel prop alıyor

Panel her sayıyı prop olarak alıyor. Yoklama zamanlayıcısı görünümün, çünkü
**container ile birlikte başlayıp durması** gerekiyor — kendi mount'unda yoklayan
bir panel, durmuş bir container'ın grafiğini hareket ettirmeye devam ederdi.

Composable'ın testi tam da bunu sabitliyor: durmuş container'da okuma
**temizleniyor, donmuyor** (donmuş bir okuma container'ın hâlâ bir şey yaptığını
iddia eder), iki kez `start` çağırmak iki zamanlayıcı bırakmıyor, ve bir örnek
alınamadığında son okuma korunmuyor.

### 32.3 Ve iki bulgu

- **Disk çubuğu sabitti.** `model-value="12"` — gerçek "R … / W …" sayılarının
  yanına çizilen, her zaman %12 gösteren bir çubuk. Ölçüm gibi duruyordu ve
  dekorasyondu. Blok G/Ç bir oran değil, iki sayaç; çizilecek bir yüzde yok.
  **Çubuk kaldırıldı** — eksiklik değil, düzeltme.
- **Bellek çubuğunun adı yoktu** — `StatCard`, `Dashboard`, `Mail` ve
  `AppearancePane`'den sonra aynı sınıfın beşinci örneği. Vuetify'ın ne
  ürettiğini bilmeden görülmüyor; axe her seferinde ilk mount'ta buluyor.

### 32.4 Kalan bölümlerin haritası

| Bölüm           | Satır | Blok |
| --------------- | ----: | ---: |
| `configuration` |   311 |    3 |
| `debug`         |   304 |    3 |
| `container`     |   271 |    3 |
| `runtime`       |   227 |    2 |
| `release`       |   112 |    1 |
| `logs`          |    13 |    1 |

Toplam 1.238 satır markup; geri kalanı başlık, araç çubuğu ve script.

### 32.5 Sayılar

|                        | §31 sonrası |      Şimdi |
| ---------------------- | ----------: | ---------: |
| `ProjectDetail.vue`    | 3.007 satır |  **2.741** |
| Frontend toplam kapsam |      %73.85 | **%75.39** |
| Frontend testleri      |         372 |    **401** |

---

## 33. §14.16 — `ProjectDetail.vue` bitti

### 33.1 Bitiş çizgisi

`ProjectDetail.vue`: **3.007 → 1.092 satır**. Sayfa artık mount edilebiliyor, ve
`tests/views-render.spec.js` onu mount ediyor — §2.3'ün "iki tanrı bileşen"
teşhisinin ikinci yarısı kapandı.

Çıkan **on dört panel** ve **dokuz composable**:

| Bölüm         | Panel                                            | Composable                  |
| ------------- | ------------------------------------------------ | --------------------------- |
| indicator     | `IndicatorPane`                                  | `useContainerStats`         |
| configuration | `OverviewPane`, `ManifestPane`, `DockerfilePane` | `useDockerfilePreview`      |
| container     | `ContainerPane`, `TunnelPane`, `WorkersPane`     | `useTunnel`, `useWorkers`   |
| logs          | `LogsPane`                                       | —                           |
| debug         | `XdebugPane`, `ProfilerPane`, `DumpsPane`        | `useXdebug`, `useProfiler`  |
| runtime       | `DevServerPane`, `PhpIniPane`                    | `useDevServer`, `usePhpIni` |
| release       | `ReleasePane`                                    | `useRelease`                |
| (ortak)       | —                                                | `useCopyTick`               |

### 33.2 İki çapraz bağ, iki olay

Panellerin çoğu bağımsız. İkisi değildi, ve ikisi de **olay** olarak çözüldü —
panelin sahibi olmadığı şeye uzanması yerine:

- `XdebugPane` toggle'ı manifest dosyasını diske yeniden yazıyor, ve aynı dosyayı
  `ManifestPane` gösteriyor. Panel `changed` yayıyor, görünüm dosyayı yeniden
  okuyor. Panelin editöre uzanması, sahiplenmediği durumu düzeltmesi olurdu.
- `ProfilerPane` ve `DumpsPane`'in "konteyneri yeniden oluştur" düğmesi projenin
  yaşam döngüsü — görünümün işi. `apply` yayıyorlar.

`useCopyTick` ise tersi bir karar: **modül kapsamlı**. Sayfa bir kopyalama için
_iki_ onay gösteriyor — düğmenin ikonu tike dönüyor, ve görünüm snackbar
kaldırıyor. Bunlar ayrı bileşenlerde; örnek-başına durum, snackbar'ı hiç
yazılmayan bir değeri izler hâlde bırakırdı.

### 33.3 Bu turun bulguları

Panelleri gerçekten mount etmek **beş** kusur çıkardı — hiçbiri lint'in
göreceği türden değil:

1. **`@/stores/ops` diye bir dosya yok.** İki panel onu import ediyordu
   (`@/stores/operations` doğrusu). ESLint import yollarını çözmüyor; hata
   yalnızca `DumpsPane` mount edildiğinde çıktı. Yani sayfa çalışma anında
   patlardı.
2. **Xdebug rozeti hiç görünmüyordu.** Ray `s.key === 'xdebug'` diye bakıyor
   ama Xdebug, profiler ve dump yakalayıcı tek `debug` sekmesinde birleştirildi
   — o günden beri "açık ama çalışmıyor" uyarısı sessiz. Rayı gezen test
   yazarken çıktı.
3. **`project_get` `null` dönerse sayfa patlıyordu.** Tipsiz sınır reject
   etmeden `null` verebiliyor; altındaki her satır alan okuyor. Kimsenin
   `await` etmediği bir async fonksiyondan fırlayan unhandled rejection ve boş
   pencere. Artık "böyle bir proje yok" durumu olarak ele alınıyor.
4. **Manifest editörünün adı yoktu.** Başlık üstünde bir `div` olduğu için
   Vuetify var olmayan bir label'a `aria-labelledby` yayıyordu: ekran okuyucu
   24 satırlık editöre ulaşıp hiçbir şey duymuyordu. axe yakaladı.
5. **Üç fikstür sözleşmeye uymuyordu** — `profiles` yerine `files`,
   `generatorVerify`'da olmayan bir `drifted` alanı, ve `preset::Plan`'in
   `rejected`/`unchanged` alanlarının eksikliği. Üçü de dokuz "unhandled
   rejection" üretiyordu; suite yeşil görünüyordu. Rust tarafındaki `Vec`
   alanları serde'de hiç yok olmaz — **fikstür savunmacı değil, sözleşme
   şeklinde olmalı**.

Ayrıca üç kopyalama düğmesi hiçbir geri bildirim vermiyordu, kardeş paneldeki
aynı düğme tik atarken. Aynı yardımcıya bağlandılar.

### 33.4 Testin kendisi de bir kez yanlıştı

İlk hâli sayfayı mount edip "bir şey render oldu mu" diye soruyordu. Yedi
bölümden beşi hiç render edilmemişken geçiyordu — varsayılan sekme dışına
çıkmıyordu. Bir paneli kasten bozmak hiçbir şeyi değiştirmedi; yakalanması bu
oldu.

Şimdiki hâli rayı geziyor ve her bölüm için **yalnızca o bölümün üretebileceği**
bir metni arıyor. Sayfa kabuğunun kendisi bir kart yığını olduğu için "bir şey
var" yetmiyor. Aynı mutasyon artık yakalanıyor.

### 33.5 Bölmenin kaçırdığı şey: stiller

Kullanıcı ekran görüntüsü gönderene kadar kimse görmedi: **çıkarılan yirmi
panelin hiçbiri stilini taşımıyordu.**

`ProjectDetail.vue`'nun ve `Settings.vue`'nun `<style scoped>` blokları
`.pane`, `.field-key`, `.section-head`, `.swatch`, `.preset-json` gibi
sınıfları tanımlıyordu. Scoped blok yalnızca _kendi bileşeninin_ render ettiği
elemanlara ulaşır — markup çocuk bileşenlere taşınınca kurallar hiçbir şeye
denk gelmez oldu. Kartların yüzeyi gitti, `.field-key`/`.field-val` çifti tek
dizeye yapıştı (`Adstackvo-parser.ajans`), Görünüm panelindeki renk kareleri
kayboldu.

**497 test bunun içinden yeşil geçti.** Mount testleri metne ve rollere bakıyor;
hiçbiri stylesheet'e bakmıyor, ve jsdom bir SFC'nin `<style>` bloğunu zaten hiç
uygulamıyor. Yani koruma bir render iddiası olamazdı — kaynağı okumak zorundaydı.

Çözüm iki paylaşılan sayfa: `src/styles/project-panes.css` (40 kural) ve
`src/styles/settings-panes.css` (14 kural), `main.js`'ten import ediliyor.
**Global değil, atadan türetilmiş** (`.detail-content …`, `.settings-scroll …`):
`.section-head` `Projects.vue`'da başka bir şey demek, `.break` ve `.mono` ise
her sayfanın kullanabileceği adlar — iki sayfa-geneli tanım kaynak sırasına
göre yarışırdı.

`tests/pane-styles.spec.js` tekrarını engelliyor: her panelin markup'ında geçen
her sınıfın ona _ulaşabilen_ bir yerde tanımlı olduğunu doğruluyor, kuralların
sahibi sayfanın altında kaldığını, ve `:deep()`'in düz CSS'e taşınmadığını —
taşınırsa selector geçersiz olur ve kural sessizce hiçbir şey yapmaz, ki bu tam
olarak bu dosyanın yakalamak için var olduğu hatanın görüntüsüdür.

Test yazılır yazılmaz iki şey daha buldu:

- **`min-w-0`** — `WorkersPane`'de kullanılıyor, ama tanım `.min-width-0`.
  Benim değil, önceden vardı: uzun bir `php artisan …` komutu düğmeyi satırdan
  itiyordu, ve yorumu bunu birebir uyarıyordu.
- **Ölü bir media query** — `Settings.vue`'da kalan `@media (min-width: 960px)`
  bloğu, artık görünümün render etmediği `.service-tabs`'i ayarlıyordu.

Alınan ders, §34.3'ün dokuzuncu uyarısı: **panel çıkarma kalıbının 2. adımı
"scoped stiller dahil" diyordu ve yirmi kez atlandı.** Bir kalıbın adımı, onu
doğrulayan bir test yoksa kalıbın parçası değildir.

### 33.6 Sayılar

|                     |  Önce |     Sonra |
| ------------------- | ----: | --------: |
| `ProjectDetail.vue` | 3.007 | **1.092** |
| Frontend testleri   |   401 |   **497** |
| Frontend kapsamı    | %75,4 | **%89,7** |
| Unhandled rejection |     9 |     **0** |
| Rust testleri       |   481 |       481 |

## 34. §14.17 — `ARCHITECTURE.md` + ADR

§12'nin ölçümü: 21 commit, 1 yazar, `CODEOWNERS`'ın her satırı aynı kişi.
"Bugün bu projeyi devralacak ikinci kişi için giriş noktası yok."

### 34.1 Ne yazıldı

- **`ARCHITECTURE.md`** — Rust tarafının dört bandı, bilinmeye değer tek istek
  akışı (`project_create`'in uçtan uca yolu), 54 modülün konuya göre tablosu,
  workspace'in disk düzeni, ve frontend'in "görünüm panel besteler, panel
  markup'a sahip olur, composable duruma sahip olur" kalıbı.
- **`docs/adr/`** — yedi karar, `README.md` indeksiyle.

Maddenin özeti şuydu: _"mevcut yorumları taşıyarak başla; yeni yazı gerekmiyor,
yalnızca yer değiştirme."_ Doğru çıktı. `elevate.rs`'in açılış paragrafı zaten
eksiksiz bir ADR — bağlam, karar, ve istenmeyen sonuç dahil. Eksik olan tek şey
**adreslenebilir** olmasıydı: numarası yok, başka yerden referans verilemiyor,
ve üzerine yazan bir ardıl tanımlanamıyor.

### 34.2 Belge de test edilir

`src-tauri/tests/architecture_claims.rs` (5 test):

- her bağlantı var olan bir dosyaya gidiyor mu;
- ADR dizini ile `ARCHITECTURE.md`'nin karar tablosu aynı kümeyi mi anlatıyor;
- her ADR'de Status, Decision **ve Consequences** var mı — sadece faydaları
  sayan bir ADR, incelenmemiş bir karardır;
- belgedeki sayılar (54 modül, 148 komut, 59 olay) ağaçla uyuşuyor mu;
- **ve ADR 0001'in kuralı**: `commands.rs` dışındaki hiçbir modül Tauri'nin
  yönetilen durumunu almıyor.

Sonuncusu maddenin kendisinden daha değerli çıktı. ADR 0001 bir yorumdu; artık
derlemeyi düşüren bir test. Ve ilk hâli **yanlıştı**: `State<'_, AppState>`
literalini arıyordu, ama ağaçta üç yazım var — kasten bozulmuş bir modül testin
tam önünde dururken geçti. `State<'_,` ile eşleşiyor artık.

### 34.3 Sınırın üçüncü tanımı

Yeni: `src-tauri/tests/contract_agreement.rs` (5 test). Sınır üç yerde
tanımlanıyor — sözleşme dosyası, `#[tauri::command]` fonksiyonları, ve
`generate_handler!` listesi — ve üçü de bugün aynı şeyi anlatıyor: 144 Rust
komutu + 3 `frontend-plugin` + 1 `deferred` = 148.

Bunu kontrol eden bir şey yoktu. `npm run contracts:check` E süiti dört kenardan
ikisini kapsıyordu, biri de derleyicinin işi. Kapsanmayan ikisi:

| Nereden             | Nereye              | Kim bakıyor                                   |
| ------------------- | ------------------- | --------------------------------------------- |
| sözleşme            | `generate_handler!` | E süiti (mevcut)                              |
| `generate_handler!` | implementasyon      | rustc                                         |
| implementasyon      | sözleşme            | **yeni** — kimsenin anlaşmadığı bir sınır     |
| implementasyon      | `generate_handler!` | **yeni** — çalışma anında `command not found` |

Ayrıca `cargo test`'e taşıyor. E süitinin belgelenmiş taban çizgisi dört hataydı;
taban çizgisi sıfır olmayan bir süit yeni bir hatada derlemeyi düşüremez.

### 34.4 Tarayıcı iki kez yanlıştı

Bu turun tek teknik zorluğu, kaynağı okuyan bir testin kendi kaynağını da
okumasıydı:

1. İlk sürüm `#[tauri::command]` özniteliğini `fn`'in hemen üstünde arıyordu.
   `workspace_pick` ve `hosts_apply` `#[tauri::command(async)]` yazılmış ve
   aralarında birer paragraf belge var — ikisi de görünmez oldu.
2. İkinci sürüm öneki eşleştiriyordu. `commands.rs` kendi kaynağını
   `#[tauri::command` için tarayan bir birim testi taşıyor, yani öznitelik orada
   bir **string literal** olarak geçiyor — tarayıcı ona kurulup bir sonraki test
   yardımcısını komut ilan etti (`generated_workspace`).

Çözüm: satırın **tamamı** öznitelik olmalı, ve eşleşme 200 satırlık bir pencere
ile sınırlı. `the_scanner_finds_a_realistic_number_of_commands` testi de bu
yüzden var — dördü de küme farkı, ve hiçbir şey bulamayan bir tarayıcı hepsini
boşuna geçirir.

### 34.5 Eskimiş bir README iddiası

`README.md` "Current baseline: **4 errors**" diyordu. Dördü de düzeltilmiş,
cümle kalmış — ve benim değişikliklerimden _önce_ de öyleydi (`git stash` ile
doğrulandı). Sayı kaldırıldı: bir metindeki rakamı hiçbir şey kontrol etmiyor,
ki bu tam olarak `readme_claims.rs`'in var olma sebebi.

### 34.6 Sayılar

|                         | Önce |             Sonra |
| ----------------------- | ---: | ----------------: |
| Rust testleri           |  533 |           **538** |
| Mimari belgesi          |  yok | `ARCHITECTURE.md` |
| ADR                     |    0 |             **7** |
| Sınırı doğrulayan kenar |  2/4 |           **4/4** |

---

## 35. Kaldığımız yer

Bu bölüm, işi devralmak için okunması gereken tek yer. §12'nin "bus factor 1"
teşhisine verilen cevabın kendisi: bir sonraki oturum — kim olursa olsun —
buradan başlar.

Artık tek yer değil, ve bu iyi haber: §34 ile `ARCHITECTURE.md` ve `docs/adr/`
var. Bu bölüm **işin nerede kaldığını** anlatır; oraya _nasıl_ yapıldığını
anlatan belge ayrıdır.

### 35.0 Bu oturum tam olarak nerede durdu

- **Ağaç yeşil.** 538 Rust testi, 533 frontend testi, `npm run lint` 0 hata,
  `npm run build` temiz, `npm run contracts:check` 0 hata / 6 uyarı, 0 unhandled
  rejection.
- **Commit edilmemiş ~55 dosya** — §33 (ProjectDetail bölme + stil düzeltmesi)
  ve §34 (mimari belgesi) birlikte. Tek commit olarak planlandı.
- **§14.18 başlandı ve geri alındı.** `src-tauri/src/policy.rs` yazıldı,
  `lib.rs`'e hiç bağlanmadı, ve `Code::Forbidden` olmadığı için derlenmiyordu.
  **Silindi**; tasarımın tamamı §35.2'de. Yarım inmiş bir modül hiç olmayandan
  kötüdür — bir sonraki oturum sıfırdan değil, yazılı bir tasarımdan başlar.
- **Bilinen ve dokunulmamış:** `stats.rs:351`'de bir clippy uyarısı
  (`iter()` yerine `values()`), bu oturumdan önce de vardı. `contracts:check`'in
  altı uyarısı da öyle — beşi henüz hiçbir görünümün çağırmadığı sarmalayıcı.

### 35.1 §14 durum tablosu

|   # | Madde                               | Durum                 | Nerede                                                             |
| --: | ----------------------------------- | --------------------- | ------------------------------------------------------------------ |
|   1 | Panic hook + crash dosyası          | ✅                    | §17.1                                                              |
|   2 | Release blokajları                  | ⚠️ **yarım**          | §17.5 — anahtar üretildi, **endpoint hâlâ 404**                    |
|   3 | SECURITY.md 404 linki               | ✅                    | §17.1                                                              |
|   4 | README'deki iki yanlış sayı         | ✅                    | §17.1, §21.5                                                       |
|   5 | Kapsam ölçümü                       | ✅ (eşiksiz)          | §17.1                                                              |
|   6 | Sürüm eşitliği + macOS imza uyarısı | ✅                    | §17.1                                                              |
|   7 | `elevate` quoting                   | ✅                    | §17.1                                                              |
|   8 | macOS sistem proxy'si               | ✅                    | §17.2                                                              |
|   9 | `ProgressSink`                      | ✅ (iki dilim)        | §18.1                                                              |
|  10 | `tauri-specta`                      | ⛔ **ertelendi**      | §18.4 — ölçüldü, ayrı dal                                          |
|  11 | `hint` i18n                         | ✅                    | §20                                                                |
|  12 | E2E                                 | ⛔ **engelli**        | §22.1 — `tauri-driver` macOS'ta çalışmıyor, Linux runner gerekiyor |
|  13 | SBOM + provenance                   | ✅                    | §21.3                                                              |
|  14 | Tanılama paketi + vitest-axe        | ✅                    | §19.5, §21                                                         |
|  15 | Bozuk prefs + `schemaVersion`       | ✅                    | §18.2                                                              |
|  16 | **Settings/ProjectDetail bölme**    | ✅ **ikisi de bitti** | §23–33                                                             |
|  17 | `ARCHITECTURE.md` + ADR             | ✅                    | §34                                                                |
|  18 | Merkezî politika + private registry | ⬜ tasarlandı         | §35.2 — `policy.rs` yazıldı ve geri alındı                         |
|  19 | Docker trait + proptest + criterion | ⬜ başlanmadı         | §35.2                                                              |
|  20 | Keystore ile sır yönetimi           | ⬜ başlanmadı         | §35.2                                                              |

**Bu tablo eksikti ve eksikliği yapısaldı.** §14, raporun _kendi gövdesinde_
teşhis ettiği her şeyi listelemiyordu: §6.3'ün sürüm kanalları, §9'un
performans ölçümü, §13'ün denetim izi gibi maddeler hiç numara almadı,
dolayısıyla burada da görünmediler ve **hiç kimse tarafından takip
edilmediler.** Beşi §36'da kapatıldı; kalanların tamamı artık numaralanmış
hâlde **§37'de** ve bu tablonun devamı orasıdır.

### 35.2 Sıradaki adım: §14.18–20

§14.16 (§33) ve §14.17 (§34) kapandı. Kalan üçü de hiç başlanmadı ve hiçbiri
diğerine bağlı değil.

#### §14.18 — Merkezî politika + private registry

Başlandı ve **kasten geri alındı**: `policy.rs` yazıldı, `lib.rs`'e hiç
bağlanmadı, ve derlenmiyordu (`Code::Forbidden` yok). Yarım inmiş bir modül
hiç olmayandan kötüdür — dosya silindi, tasarım buraya yazıldı.

**Politika dosyası.** Yalnızca yöneticinin yazabileceği tek bir JSON:

| Platform | Yol                                                     |
| -------- | ------------------------------------------------------- |
| macOS    | `/Library/Managed Preferences/com.stackvo.desktop.json` |
| Windows  | `%ProgramData%\StackVo\policy.json`                     |
| Linux    | `/etc/stackvo/policy.json`                              |

`STACKVO_POLICY_FILE` üçünü de geçersiz kılar. Bu bir arka kapı değil, testin
oraya ulaşmasının tek yolu — ve **açıkça söylenmeli**: bu değişkeni
ayarlayabilen kullanıcı kendi yazdığı dosyayı da gösterebilir. Katman, iş
birliği yapan bir uygulamaya kurumun _niyetini_ bildirir; **güvenlik sınırı
değildir** ve öyle anlatılmamalıdır.

**Neden JSON, platformun kendi deposu değil.** macOS MDM `.plist`, Windows Group
Policy registry anahtarı yazar. İkisini okumak bu crate'te olmayan iki ayrı
ayrıştırıcı demek — ve iki mekanizma da bir dosyayı bir anahtar kadar kolay
dağıtabiliyor. Tek biçim, üç yol, tek ayrıştırıcı. plist/registry okuyucuları
bariz bir sonraki adım ve tahmin edilmemeli.

```json
{
  "schemaVersion": 1,
  "settings": { "DEFAULT_TLD_SUFFIX": "corp.test", "SERVER_TYPE": "nginx" },
  "locked": ["DEFAULT_TLD_SUFFIX"],
  "registryPrefix": "registry.corp.example/proxy"
}
```

Kurallar, yazılırken çıkanlar dahil:

- **Öncelik**: gömülü varsayılan < `.env` < politika. `config::Env::load`'un
  sonuna bir `policy::apply`.
- **Ayarlamadığı bir anahtarı kilitleyemez.** "Neye" demeden "değişmesin"
  demek, makineyi elindekinde bırakmaktır. Böyle bir giriş yok sayılır ve
  `error` alanında bildirilir.
- **Bozuk politika uygulamayı açılmaz yapmaz.** Dağıtılmış bir dosyadaki yazım
  hatası, açılmayan bir filo demek olmamalı — boş politika + `error` döner, ve
  hata _bildirilir_, yutulmaz: sessizce hiçbir şey yapmayan bir politika,
  onu dağıtan yöneticinin yürürlükte sandığı bir politikadır.
- **Kilitli anahtara yazma** `env_set`'te reddedilir; hata politikanın hangi
  dosyadan geldiğini söyler, ve Settings paneli alanı düzenlenebilir değil
  _yönetiliyor_ diye çizer.

**Mirror kuralı** (`policy::mirror`), üç istisnayla:

- zaten bir registry adı taşıyan referans (`ghcr.io/x/y`, `localhost:5000/z`) —
  Docker'ın tanıdığı kuralla: ilk parça `.` ya da `:` içeriyorsa veya tam olarak
  `localhost` ise. Kasıtlı bir seçimi başka yere yönlendirmek yanlış olur.
- zaten önekle başlayan referans — ikinci render ikinci yeniden yazma olmamalı.
- **`stackvo-` ile başlayan imaj** — bunlar bu makinede `docker compose build`
  ile üretiliyor ve hiçbir registry'de yok. Önek eklemek onları aynı anda hem
  çekilemez hem inşa edilemez yapar.

Yeniden yazma **render edilmiş metin** üzerinde yapılmalı, yirmi `.tpl`
dosyasında değil: şablonlar Bash üreticisiyle olan sözleşme, Bash mirror'dan
haberdar değil, ve dosyaları düzenlemek her differential karşılaştırmayı porta
ilgisiz bir sebeple düşürür.

Ayrıca gerekenler: `Code::Forbidden` (bugün yok), `policy_status` komutu +
sözleşme girişi, Settings'te "yönetiliyor" rozeti, ve ADR 0008.

#### §14.19 — Docker trait + proptest + criterion

`ProgressSink` (§18.1) bunun küçük provası: Tauri'siz bir soyutlama, test
edilebilir ikizlerle. `engine.rs` bugün bollard'ı doğrudan çağırıyor, yani
"daemon şunu döndürürse ne olur" hiçbir yerde test edilemiyor.

#### §14.20 — Keystore ile sır yönetimi

v2 sözleşme değişikliği olarak planlanmalı (§5.2).

#### Hâlâ engelli olan ikisi

§14.10 `tauri-specta` (ayrı dal, ADR 0006'nın ardılı) ve §14.12 E2E (Linux
runner gerekiyor).

### 35.3 Devralan için dokuz uyarı

1. **`Settings.vue` %0 kapsamda ve öyle kalacak** — bölme bitene kadar. Bu bir
   gerileme değil, ölçünün dürüst hâli.
2. **Mount testleri gerçek hata buluyor.** Bu oturumda dört ayrı sınıf çıktı:
   tipsiz IPC sınırı (§22.3), `hintKey`'in düşmesi (§22.4), erişilebilirlik
   (§22.5), ve bozuk i18n dizeleri (§24.3). Bir paneli çıkarırken çıkan hatayı
   _susturmayın_ — o, çıkarmanın getirisidir.
3. **Paneli, uygulamada yaşadığı bağlamda mount edin.** §27.2: `DomainPane`
   çıplak mount edildiğinde dört iddia düştü ve dördü de doğru davranıştı —
   paylaşılan editör yüklenmemişti. Testin kurgusu yanlışsa test yanlış soruyu
   sorar.
4. **Dosyayı değiştirdikten sonra dilimlerini yeniden alın.** §31.3: bir
   panel çıkarıldıktan sonra satır numaraları kaydı, önceden alınmış dilimler
   yanlış içerik taşıdı, ve bir bileşen başka bir panelin markup'ıyla üretildi.
5. **Bir bloğu silmeden önce ne sildiğini doğrulayın.** §26.2: bir betik
   niyetlenilenin ötesine geçip dört sabit sildi, şablon onları kullanmaya
   devam etti, ve 265 testin hiçbiri görmedi. `vue/no-undef-properties` artık
   açık — ama kesme sınırını yine de `assert` edin.
6. **Testte tek bir Pinia kullanın**, ve store'u `useStore(pinia)` ile açıkça
   çözün. §28.3: `mount`'a verilen pinia ile testin okuduğu ayrı çıktı, durum
   görünmez oldu ve iddia sebepsiz düştü.
7. **Fikstürü sözleşme şeklinde yazın, savunmacı değil.** §33.3: Rust'ta
   `Vec<T>` olan bir alan serde'de hiç yok olmaz, ama üç fikstür onları atladı
   ve dokuz sessiz unhandled rejection üretti. Eksik fikstür, arka ucun
   gönderemeyeceği bir yükü test etmektir.
8. **Stilleri taşımayı unutmayın — ve bunu bir testle doğrulayın.** §33.5:
   kalıbın 2. adımı "scoped stiller dahil" diyordu, yirmi kez atlandı, ve 497
   test görmedi. `tests/pane-styles.spec.js` artık bekçi.
9. **`git stash` ile "bu benim mi?" kontrolü yapın.** Bu oturumda üç kez
   kullanıldı ve üçünde de cevap "hayır, önceden vardı" idi (§19.1, §19.2).
   Kaydedilmemiş bir hatayı kendi değişikliğine yazmak, iki turu boşa harcar.

### 35.4 Sahibine kalanlar — hâlâ açık

§17.5 değişmedi:

1. **Güncelleme endpoint'i 404** — `tauri.conf.json` `stackvo/stackvo-tauri`'yi
   gösteriyor, o repo yok. **Uygulama hâlâ güncelleme alamaz.**
2. **`TAURI_SIGNING_PRIVATE_KEY`** GitHub secret'ı girilmedi. Özel anahtar
   `~/.tauri/stackvo.key`'de (mod 600, repoya girmedi), parolasız.
3. **Apple / Windows imzalama secret'ları** girilmedi — artık eksikse release
   log'unda uyarı çıkıyor (§17.1), ama hâlâ eksikler.
4. ~~**Kapsam eşiği** yok.~~ **Kapandı — §36.1.** Eşik kondu, CI kapısı var.

---

## 36. Takip listesinde olmayan beş madde

Bu turun konusu §14 listesi değil. §14 mühendislik borcunu sayıyordu; raporun
**gövdesinde** teşhis edilip §14'e hiç girmemiş, dolayısıyla §35.1 durum
tablosunda da görünmeyen — yani kimsenin takip etmediği — maddeler vardı. Beşi
kapatıldı.

Ortak özellikleri: hiçbiri "özellik" değil, hepsi **bir iddianın doğru
kalmasını sağlayan mekanizma.** Bu yüzden beşi de bir dosya değil, bir dosya
artı onu koda bağlayan bir kapı olarak indi.

### 36.1 Kapsam eşiği _(§14.5'in yarım bıraktığı yarısı, §17.5.4, §35.4.4)_

§14.5 "eşiksiz başla, sadece gör" diyordu ve haklıydı: 30,70%'te konan bir
eşik ya boşluğu onaylar ya ilk koşuda düşer. O gerekçenin bir son kullanma
tarihi vardı ve geçti — sayılar dört tur izlendi (Rust 61,60 → 64,07; frontend
30,70 → 89,70).

`tools/coverage-floors.mjs` tek kaynak: `vitest.config.js` eşikleri oradan
okuyor, CI'ın yeni "Hold the floors" adımı da Rust raporunu ona karşı tutuyor.
Politikanın iki yerde yazılması, bir gün ikisinin farklı şeyler söylemesi
demekti.

| | Ölçülen | Eşik | Payın gerekçesi |
| --- | ---: | ---: | --- |
| Rust satır | %64,07 | **%60** | Ölçüm macOS'ta, CI Ubuntu'da — `cfg` dalları farklı derleniyor |
| Frontend satır | %89,70 | **%85** | Testleri henüz yazılmamış bir modülün ilk commit'i düşürmesin |
| Frontend dal | %77,17 | **%72** | aynı |

**`functions` bilinçli olarak eşiksiz.** v8 her ok fonksiyonunu sayıyor ve bir
SFC şablonu render fonksiyonu içinde onlarca üretiyor; %53 "frontend'in yarısı
test edilmemiş" demek değil, "bir derleyicinin ürettiği kapanışların yarısı
jsdom'dan çağrılamıyor" demek. Üzerine eylem alınamayan bir sayıya eşik koymak,
insanlara kırmızı bir kapıyı görmezden gelmeyi öğretir.

Eksik rapor **geçmiyor, düşüyor**: ölçüm adımları `continue-on-error` olduğu
için "rapor üretilmedi" durumu sessizce yeşil olabilirdi, ki görmediği için
geçen bir kapı hiç olmayandan kötüdür.

### 36.2 Gizlilik beyanı _(§4.3)_

Rapor "opt-in telemetri ya da 'telemetri yoktur' satırı — ikisi de kabul
edilebilir, belirsizlik değil" demişti. Belirsizlik on ay durdu.

`PRIVACY.md`: ne saklanıyor (dosya, yol, süre), ne çıkıyor (uygulamanın kendi
inisiyatifiyle **yalnızca iki adres**: güncelleme endpoint'i ve loopback'teki
mail catcher), kullanıcının isteğiyle ne çıkıyor, Docker imaj derlemesi
sırasında ne indiriliyor. Artı korumadığı şeyler — `.env`'deki düz metin
şifreler, tünelin siteyi **herkese açık** hâle getirmesi.

Asıl iş beyan değil, **kapı**: `src-tauri/tests/privacy_claims.rs` üretim
kodundaki (ve frontend'deki, ve `tauri.conf.json`'daki) her `http(s)://`
adresini tarıyor ve `PRIVACY.md`'de adı geçmeyen bir host bulursa build'i
kırıyor. Yer tutucular, loopback, `.loc`/`.test` ve RFC 2606 adresleri
**kural** ile eleniyor — "görmezden gelinecek hostlar" listesiyle değil, çünkü
öyle bir listenin onuncu satırı düşünülmeden eklenir ve önemli olan o olur.

Mutasyonla denendi: `mail.rs`'e gizlice bir `https://metrics.…/collect`
konduğunda test onu adıyla bildiriyor.

### 36.3 NOTICE — ve neden repoda durması yetmiyor

§13'ün "üçüncü taraf lisans bildirimi: yok" satırı. MIT, BSD, ISC ve
Apache-2.0'ın hepsi bildirimin **yazılımla birlikte yolculuk etmesini**
istiyor; `.dmg` alan kişi repoyu görmüyor, dolayısıyla depodaki bir dosya bu
yükümlülüğü karşılamıyor.

- `tools/generate-notice.mjs` — `Cargo.lock` ve `package-lock.json`'dan üretiyor:
  **572 Rust crate + 40 npm paketi**, lisans metinleri ve telif satırlarıyla.
- `NOTICE.md` `include_str!` ile **ikiliye derleniyor** (`licences.rs`), bundle
  resource olarak değil: resource, çalışma anında çözülen ve hiçbir şeye
  çözülebilen bir yol; derlenmiş bildirim ya oradadır ya build düşmüştür.
- Yeni `licences_notice` komutu + About penceresinde okunabilir bir panel.
- `npm run notice:check` CI'ın supply-chain job'ında: bir bağımlılık bildirimsiz
  geldiğinde adıyla düşüyor.

**Üretecin ilk iki sürümü yanlıştı ve ikisi de ölçümle yakalandı.** Kilit
dosyasının `dev` bayrağına bakan sürüm 107 paket sayıyordu — 28 `@esbuild/*` ve
24 `@rollup/rollup-*` platform ikilisi dahil, yani bir bundler'ın parçaları
"kullanıcıya giden" listesindeydi. Grafiği yürüyen sürüm 13 saydı, çünkü
çözücü üst seviye `node_modules`'a bakmayı unutuyordu. Doğrusu 40. Bir sayının
makul görünmesi doğru olduğu anlamına gelmiyor; ikisi de makul görünüyordu.

### 36.4 ADR 0008 — kırıcı sözleşme değişikliği nedir

§12: `contractVersion` alanı var, neyin major sayıldığı **tanımsız**. Tanımsız
bir sürüm numarası, kimsenin geriye doğru okuyamadığı bir süstür.

ADR 0008 kuralı yazıyor, ve kural prose olarak kalmıyor:
`contracts/surface.lock.json` **son yayınlanan** çağrı yüzeyini tutuyor,
`src-tauri/tests/contract_version.rs` her `cargo test`'te farkı sınıflandırıyor
ve `contractVersion` yetmiyorsa build'i kırıyor — hangi komutun, hangi
argümanın, hangi alanın istediğini söyleyerek.

Bu tur onu gerçek bir vakayla denedi: `licences_notice` eklendi → kapı
"contractVersion 1.0.0, en az 1.1.0 olmalı, sebep: `licences_notice` yeni"
dedi → sürüm **1.1.0**'a çıktı.

Yan kazanç: ADR 0006'nın açıkça "güvene bırakıldı" dediği yarı kapandı.
`contract_agreement.rs` komut **kümesini** koda karşı tutuyordu; şekilleri
kimse tutmuyordu, yani `Project`'ten düşen bir alan hiçbir komutun `returns`
değerini değiştirmediği için sessizce geçerdi. Adlandırılmış tipler artık alan
alan karşılaştırılıyor.

Sınırı da yazmak gerekiyor: bu, **sözleşmeyi sözleşmeye** karşı tutan bir
kontrol. Rust struct'ından düşen ama sözleşmeden düşmeyen bir alan burada da
görünmez — o boşluğu kapatan şey `tauri-specta` (§14.10).

### 36.5 platform-matrix yeniden ölçüldü _(§D)_

Doküman yanlış yazılmamıştı; **bayatlamıştı**. 142 komut derken 149, 47 dosya
derken 95, 32.515 satır derken 37.914 vardı. Prose'daki bir sayının yaşlanmaya
karşı hiçbir savunması yok — ölçüm olmaktan çıkıp bir ölçümün hatırası oluyor,
ve okuyucu hangisine baktığını ayırt edemiyor.

Her sayı yeniden sayıldı ve `src-tauri/tests/platform_matrix_claims.rs` ile
koda bağlandı. En değerlisi bir sayı değil, bir **özellik**: `invoke(` kelimesi
`ipc.js` dışında sıfır yerde geçiyor — tüm web argümanı buna dayanıyor, ve
ikinci bir `invoke(` bulguyu yanlış yapar.

Elle sınıflandırılan dört satır (bollard / compose / dosya sistemi / ayrıcalık)
gate dışında bırakıldı **ve yöntemleri yazıldı**. Bunlar kodun ne _anlama_
geldiğine dair yargılar; bir testin onları çözüyormuş gibi yapması, önlemek
için var olduğu bayat sayıdan daha kötü olurdu.

### 36.6 Yolda çıkan dört gerçek hata

Beş maddenin hiçbiri hata avı değildi. Dördü yine de çıktı — ve dördü de
"yanlış olduğunda hiçbir şeyin şikâyet etmediği" sınıfından:

1. **`npm run test:js` aralıklı kırmızıydı** — ölçüldü: sekiz koşunun üçü ile
   beşi arası. `git stash` ile doğrulandı, benim değişikliğim değil (§35.3.9
   kuralı). **İki ayrı sebebi vardı ve ilk teşhis yarımdı:**

   - **`App.vue`'nun async `onMounted`'i.** `boot()` beklenirken bileşen unmount
     edilirse `onUnmounted` çoktan koşmuş oluyor ve **sonra** `metrics.start()`
     çalışıyor; sahipsiz kalan iki saniyelik zamanlayıcıyı artık hiçbir şey
     temizleyemiyor. Aynı sınıf `listenAll` handle'larını da sızdırıyordu.
     Düzeltme: `disposed` bayrağı + `keep()`. Regresyon testi mutasyonla denendi.
   - **Asıl hacim: `app-shell.spec.js`'in kendisi.** On bir test shell mount
     edip hiç unmount etmiyordu; her biri suite'in geri kalanı boyunca 2 ve 5
     saniyede bir yoklamaya devam ediyor, sonra yıkılmış bir `document` üzerinde
     patlıyordu. İlk düzeltmeden sonra da düşmeye devam etti — **ve benim yeni
     testim, tam olarak bu sızıntı yüzünden düştü.**

   İkinci sebep ancak yığın izi alınarak bulundu (`Timeout._onTimeout →
   metrics.js:74`), çünkü dosya tek başına koştuğunda hiç düşmüyor: sızan
   zamanlayıcının ateşlenmesi için suite'in yavaş olması gerekiyor, ki bu da
   yalnızca tam koşuda oluyor. Temizlik on bir teste `unmount()` eklenerek
   değil, `mountShell()`'in mount ettiğini kaydedip `afterEach`'in hepsini
   indirmesiyle yapıldı — aksi hâlde yazılacak bir sonraki test on ikincisi
   olurdu. **Sekiz ardışık tam koşu temiz.**
2. **"59 olay" on aydır yanlıştı.** Sözleşmenin `events` nesnesindeki `_note` ve
   `_removed` bölüm yorumları olay olarak sayılıyordu — gerçek sayı **57**. Asıl
   kayda değer olan: `architecture_claims.rs` bunu doğruluyordu ve **aynı hatayı
   yapıyordu**. Belgenin hatasını paylaşan bir kapı, ikinci bir görüş değildir.
3. **NOTICE üretecinin iki yanlış sürümü** (§36.3).
4. **Kendi satır sayım hatam.** platform-matrix'e 37.969 yazmıştım; kapı
   kurulur kurulmaz 37.914 olduğunu söyledi. Fark tam olarak 55 — modül başına
   bir fazladan satır, yani sayma yönteminin kendi hatası. Kapının ilk
   yakaladığı şeyin onu yazan kişi olması, kapının çalıştığının kanıtı.

### 36.7 Sayılar

| | Önce | Sonra |
| --- | ---: | ---: |
| Rust testleri | 538 | **556** |
| Frontend testleri | 533 | **536** |
| Rust satır kapsamı | %64,05 | **%64,07** (eşik %60) |
| Frontend satır kapsamı | %89,65 | **%89,70** (eşik %85) |
| IPC komutu | 148 | **149** |
| `contractVersion` | 1.0.0 | **1.1.0** |
| ADR | 7 | **8** |
| Belgeyi koda bağlayan test dosyası | 2 | **5** |
| Aralıklı düşen frontend suite | ~%40 | **hayır** (8 ardışık temiz tam koşu) |

Doğrulama: `cargo test` 556/556, `cargo clippy -D warnings` temiz,
`cargo fmt --check` temiz, `npm run lint` 0, `npm run build` temiz,
`npm run notice:check` 612 paketi kapsıyor, `npm run coverage:floors` dört
eşiği de geçiyor. `npm run contracts:check` **bu makinede koşturulamadı** —
`../stackvo` checkout'u yok (§2.1'in kaydettiği harici bağımlılık); CI'da
koşuyor.

---

## 37. Kalan işlerin tam listesi

Bu bölüm §35.1'in devamı ve raporun **tek açık iş kuyruğu**. Var olma sebebi
§35.1'in altında yazılı: §14 listesi, raporun kendi gövdesinde teşhis edilen
her şeyi numaralandırmamıştı, ve numarası olmayan madde takip edilmiyor. Sürüm
kanalları (§6.3) on ay boyunca tam olarak bu yüzden hiçbir listede görünmedi —
kusur olarak değil, **hiç sayılmamış** olarak.

Kural: bir madde ancak buradan çıkarılabilir, ve çıkarken §36 gibi bir uygulama
kaydı bırakır.

### 37.1 Durum tablosu

Numaralar §14'ün devamı. "Doğrulandı" sütunu, maddenin bugün hâlâ açık
olduğunun bu turda ağaca karşı kontrol edildiğini söyler.

|   # | Madde                                              | Kaynak | Durum          | Doğrulandı                                              |
| --: | -------------------------------------------------- | ------ | -------------- | ------------------------------------------------------- |
|   2 | Güncelleme endpoint'i                              | §6.1   | ⚠️ **blokaj**  | `latest.json` → HTTP 404; repo yok                      |
|  10 | `tauri-specta` ile tip üretimi                     | §2.2   | ⛔ ertelendi   | `Cargo.toml`'da specta izi yok                          |
|  12 | E2E (`tauri-driver`)                               | §3.2   | ⛔ engelli     | Repoda driver/wdio/playwright yok; Linux runner gerek   |
|  18 | Merkezî politika + private registry ön eki         | §13    | ⬜ tasarlandı  | `policy.rs` yok (§35.2'de tasarım hazır)                |
|  19 | Docker trait + `proptest` + `criterion`            | §3.3–4 | ⬜ başlanmadı  | `benches/` yok, üç crate de bağımlılıklarda yok         |
|  20 | Keystore ile sır yönetimi                          | §5.2   | ⬜ başlanmadı  | `keyring` bağımlılıklarda yok                           |
|  21 | **Sürüm kanalları, kademeli dağıtım, geri alma**   | §6.3   | ⬜ başlanmadı  | Tek `latest.json`, tek kanal, geri alma yolu yok        |
|  22 | Platform kapsamı ve paketleme                      | §6.4   | ⬜ başlanmadı  | `release.yml` dört hedef: Linux aarch64 ve Win ARM64 yok |
|  23 | Tray/menü etiketleri Rust'ta sabit                 | §7.2   | ⬜ başlanmadı  | `lib.rs:115` hâlâ `== "tr"` boolean'ı                   |
|  24 | RTL                                                | §7.3   | 🟡 yarım       | Bayrak ve taşıma var; Vuetify `rtl` yapılandırması yok  |
|  25 | Erişilebilirlik beyanı (VPAT / EN 301 549)         | §8     | ⬜ başlanmadı  | Beyan yok; §14.12 olmadan üretilemez                    |
|  26 | Performans bütçesi: `criterion` + `size-limit`     | §9     | ⬜ başlanmadı  | Benchmark yok, bundle bütçesi yok                       |
|  27 | Sıcak yollar: `list_projects` cache, gizli pencere | §9     | ⬜ başlanmadı  | Cache yok; `is_visible()` kod tabanında hiç geçmiyor    |
|  28 | `stats_history` kalıcılığı                         | §10    | ⬜ başlanmadı  | `commands.rs:42` hâlâ `Mutex<HashMap>`, süreç ömürlü    |
|  29 | Mutex poisoning                                    | §10    | ⬜ başlanmadı  | `commands.rs`'te 14 `lock()`; `parking_lot` yok         |
|  30 | Denetim izi (audit log)                            | §13    | ⬜ başlanmadı  | Ayrı, döndürülmeyen bir audit log yok                   |
|  31 | Air-gapped kurulum                                 | §13    | ⬜ başlanmadı  | Offline imaj paketi yolu yok                            |
|  32 | Destek / sürüm ömrü politikası                     | §13    | ⬜ başlanmadı  | SECURITY.md: "yalnızca en son"                          |
|  33 | Sözleşme kapısının kalan harici bağımlılığı        | §2.1   | 🟡 yarım       | `ci.yml:212` hâlâ `stackvo/stackvo` checkout'u yapıyor  |
|  34 | Web sürümü / HTTP ikilisi                          | matris | ⬜ başlanmadı  | `src/bin/` yalnızca `stackvo-mcp.rs`                    |
|  35 | Windows ve Linux dallarının çalıştırılması         | matris | ⬜ başlanmadı  | UAC, polkit, ConPTY, `certutil` hiç koşturulmadı        |

Kapananlar bu tabloda yok: §36'daki beş madde (kapsam eşiği, gizlilik beyanı,
NOTICE, sözleşme sürüm politikası, matrisin yeniden ölçümü) ve §14'ün 1, 3–9,
11, 13–17 numaralı maddeleri.

### 37.2 §14.21 — kanal işleri, ayrıntısıyla

Kullanıcının adıyla sorduğu madde bu ve listede olmamasının bedeli somut:
**bugün kötü bir sürüm çıkarsa yapılabilecek tek şey yeni bir sürüm çıkarmak,
o da güncelleme almış herkese anında gidiyor.** Geri alma yok, yavaşlatma yok,
durdurma yok.

Bugünkü durum, doğrulanarak: `tauri.conf.json` → tek `endpoints` girdisi, tek
`latest.json`, kanal kavramı yok.

Yapılacak iş, artan maliyetle:

1. **Acil durdurma anahtarı** — en ucuzu ve en değerlisi. `latest.json`'a bir
   alan (`"paused": true`) ve istemci tarafında ona bakan bir kontrol; bir
   sürümün dağıtımını **yayınlamadan** durdurmayı mümkün kılar. Bir günlük iş,
   ve diğer üçünün ön koşulu değil.
2. **Kanallar** (`stable` / `beta`). Tauri updater endpoint şablonunu
   destekliyor; `latest-{{channel}}.json` ve tercihte bir kanal seçici. Kanalın
   kullanıcı tercihi olması gerekiyor, ve §14.18'in politika katmanı geldiğinde
   **kilitlenebilir** olması (kurumsal ihtiyaç: "güncelleme kanalı kilitli").
3. **Kademeli dağıtım.** `latest.json`'da bir yüzde alanı, istemcide kararlı
   bir hash (makine kimliği değil — gizlilik beyanına yeni bir alan eklememek
   için kurulum başına rastgele, kalıcı bir sayı yeter). Yüzde dışındaki
   istemci güncellemeyi görmez.
4. **Geri alma.** `latest.json`'ın daha eski bir sürümü göstermesi tek başına
   yetmez: Tauri updater sürüm karşılaştırması yapıyor ve aşağı inmez.
   Gerçek geri alma, "bu sürümü durdur" + yeni bir yama sürümü demektir — yani
   (1) olmadan geri alma diye bir şey yok.

**Bağımlılık:** dördü de §14.2'nin arkasında. Endpoint 404 olduğu sürece kanal
mantığı yazılabilir ama **çalıştırılamaz**, ve bu raporun kendi kuralına göre
(§22.1) çalıştırılamayan altyapı gönderilmez.

### 37.3 Bir haftalık olanlar

§14'ün ilk sekizi gibi, ucuz ve birbirine bağlı olmayanlar:

- **§14.32 destek politikası** — SECURITY.md'ye bir paragraf. "Yalnızca en son"
  bugünkü gerçek; yazılı olması kurumsal satın almada sorulan şey.
- **§14.29 mutex poisoning** — `parking_lot` ya da sekiz çağrı yerinde bilinçli
  kurtarma. `prefs_set`'in `unwrap_or_else(|e| e.into_inner())` deseni zaten
  doğru olanı; kalanlar ona hizalanır.
- **§14.27'nin yarısı** — `if window.is_visible()` ile arka plan yoklama
  aralığını uzatmak. Tek koşul, ölçülmemiş bir pil maliyetini kaldırır.
- **§14.23** — tray/menü etiketlerini `tray_relabel` üzerinden frontend'in
  beslemesi. Komut zaten kayıtlı; üçüncü dilin kod değişikliği olmaktan çıkması
  buna bağlı.
- **§14.21.1** — yukarıdaki durdurma anahtarı, endpoint ayağa kalktığı gün.

### 37.4 Bu listenin kendisi nasıl doğru kalır

§36'nın beş maddesi de bir belgeyi koda bağlayan bir kapı bıraktı. Bu liste
öyle bir kapı **bırakamaz**: "yapılmadı" bir kodun ölçülebilir özelliği değil,
bir niyetin kaydı. Elde olan tek şey, her satırın **bugün doğrulanmış** olması
ve neyin bakılarak doğrulandığının yazılı olması — bir sonraki oturum tabloyu
okumak yerine aynı kontrolleri tekrarlayabilir.

Bu, §11'in tezinin sınırıdır ve yazılması gerekir: doğrulanabilir olan her şey
doğrulandı; bu tablo doğrulanabilir olanın dışında kalan kısımdır.
