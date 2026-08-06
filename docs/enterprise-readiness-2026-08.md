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

| Alan | Puan | Tek cümlelik gerekçe |
| --- | :-: | --- |
| Kod kalitesi & gerekçelendirme | **9/10** | Yorumlar karar kaydı seviyesinde; sektörde nadir. |
| Hata yönetimi (Rust) | **9/10** | Üretim kodunda **7** `unwrap/expect` — hepsi bilinçli invariant. |
| Sözleşme bütünlüğü (kod) | **9/10** | Ölçüldü: kayıt ↔ implementasyon ↔ sözleşme **sıfır drift**. |
| Güvenlik modeli (tasarım) | **8/10** | argv-only spawn, yol sınırlama, dar capability, allowlist'li URL. |
| Tedarik zinciri | **7/10** | `cargo-deny` + Dependabot + `npm audit`; SBOM ve provenance yok. |
| Test **stratejisi** | **5/10** | 478 test var ama kapsam ölçülmüyor, E2E yok, sıcak modüller zayıf. |
| Gözlemlenebilirlik | **5/10** | Mükemmel log altyapısı, ama panic hook yok ve `panic = "abort"`. |
| Mimari katmanlama | **4/10** | 6.195 satırlık tek `commands.rs`; 48 komut `AppHandle`'a yapışık. |
| Sürüm mühendisliği | **3/10** | `pubkey: ""` **ve** güncelleme endpoint'i 404. Dağıtılamaz. |
| Tip güvenliği (uçtan uca) | **3/10** | IPC sınırında tip yok; 22k satır JS Rust'ın struct'larını bilmiyor. |
| Dokümantasyon doğruluğu | **3/10** | README'de iki ölçülebilir iddia yanlış; SECURITY.md linki 404. |
| i18n mimarisi | **5/10** | Kod tarafı doğru tasarlanmış; 146 spesifik dize çevrilmiyor. |
| Erişilebilirlik | **3/10** | Tek regex testi; axe yok, klavye/focus testi yok, RTL yok. |
| Performans mühendisliği | **2/10** | Tek benchmark yok, bundle bütçesi yok, cache stratejisi yok. |
| Yönetişim & süreklilik | **2/10** | Bus factor 1; ADR yok, ARCHITECTURE.md yok. |
| Kurumsal dağıtım | **1/10** | Merkezî politika, private registry, air-gap — hiçbiri yok. |

**Teşhis.** Bu, *tek bir çok iyi mühendisin* yazabileceği en iyi kod
tabanlarından biri — ve tam olarak o yüzden kurumsal değil. Eksikler kod
kalitesinde değil; **kalitenin kod dışına, otomatik ve devredilebilir hale
çıkarılmasında.** Kod içinde doğrulama var (E/F suite'leri, differential
testler); kodun *çevresinde* — README, release, dağıtım, devir — yok.

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
   *(ölçüm: her dosyada `#[cfg(test)]` öncesi bölüm)*, üçü `contracts.rs`'te
   derlenmiş JSON'un parse'ı. Çoğu Rust projesinin geçemediği bir çıta.
4. **Sözleşme bütünlüğü — ölçüldü, sıfır drift.**
   `lib.rs` **143** komut kaydediyor, `commands.rs` **143** komut uyguluyor,
   fark **yok**. `contracts/ipc.json` 147 komut bildiriyor: 143 Rust komutu +
   3 `kind: "frontend-plugin"` (bilinçli olarak frontend'de) + 1
   `status: "deferred"` (`updates_check`). Sözleşme kendi istisnalarını
   *makine-okunur alanlarla* işaretliyor. Bu, bu incelemede rastlanan en
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

**Bugün drift yok** (§1.4) — bu bir *risk* bulgusudur, bir kusur değil. Ama
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
*kopyasını* test içinde yeniden kuruyor, sonra gerçek dosyayı metin olarak
okuyup kopyanın hâlâ eşleştiğini doğruluyor ("shape mirror" tekniği).

Bu, tanrı bileşeni test etmenin *yaratıcı* bir çözümü ve yorumları neden böyle
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

| Modül | Satır | Test | Not |
| --- | --: | --: | --- |
| `engine.rs` | 1.391 | 4 | Docker'a dokunan her şeyin merkezi |
| `pty.rs` | 501 | 4 | Kullanıcı makinesinde süreç açıyor |
| `scaffold.rs` | 791 | 5 | 28 şablon, hepsi kullanıcı dosyası yazıyor |
| `watcher.rs` | 193 | 4 | Dosya sistemi olayları |
| `error.rs` | 134 | 0 | Serileştirme şekli sözleşmenin parçası |

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

Modülün kendi yorumu bunu kabul ediyor: *"her çağıran ne geçirdiğinden
sorumlu."* İki çağıran da uygulama yollarından kuruyor — ama o yollar kullanıcı
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
zorlaştırır — bu bir *v2 sözleşme değişikliği*, ama şimdi planlanmalıydı.

### 5.3 Tedarik zinciri: SBOM ve provenance yok

`cargo-deny` + Dependabot + `npm audit` iyi bir taban. Eksik olan üçü de
kurumsal satın almada **sorulan** şeyler:

- **SBOM** (CycloneDX/SPDX) — `cargo cyclonedx` + `npm sbom`, CI'da beş satır.
- **Build provenance** (SLSA) — `actions/attest-build-provenance`, üç satır.
- **Artefakt checksum'ları** — `latest.json` imzalı, ama manuel indiren için
  SHA-256 listesi yok.

### 5.4 macOS sistem proxy'si okunmuyor *(düzeltilmiş bulgu — §15)*

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

## 7. i18n — tasarım doğru, kapsama eksik *(düzeltilmiş bulgu — §15)*

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

Bu projenin tezi şu: *"'muhtemelen aynı' shipping için bir standart değil."*
Kod bu teze uyuyor (E/F suite'leri, differential testler, `mcp.rs`'te tool ↔
komut çapraz kontrolü). **README bu tezin dışında kalmış tek yüzey** — ve
ölçülebilir iki iddiası bugün yanlış:

| README iddiası | Ölçülen | Fark |
| --- | --- | --- |
| *"Thirty-four commands take an `AppHandle`"* (satır 152) | **48** | +14 |
| *"Two tools change things (Xdebug…, reissuing the certificate)"* (satır 139) | **17 araçtan 7'si** `writes: true` | +5 |

Yedi yazma aracı: `xdebug_set`, `certificates_reissue`, `project_start`,
`project_stop`, `stack_up`, `stack_down`, `generate`. README yalnızca ilk
ikisini sayıyor — yani `--allow-writes` bayrağının bir MCP istemcisine
verdiği yetkinin **stack'i tümüyle durdurmayı içerdiği** dokümantasyonda
yazmıyor. Bu bir *güvenlik dokümantasyonu* boşluğu, tipografik bir hata değil.

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

| İhtiyaç | Bugün | Olması gereken |
| --- | --- | --- |
| Merkezî konfigürasyon | yok | MDM / Group Policy / `/Library/Managed Preferences` okuyan, kullanıcı ayarını override eden politika katmanı |
| Zorunlu/kilitli ayarlar | yok | "Güncelleme kanalı kilitli", "telemetri kapalı", "workspace şurada" |
| Private Docker registry | yok | Şablonlardaki `image:` referansları için kurumsal mirror ön eki |
| macOS sistem proxy'si | okunmuyor (§5.4) | `macos-system-configuration` özelliği + görünür hata |
| Air-gapped kurulum | yok | Şablonlar zaten binary'de; imajlar Docker Hub'dan, offline bundle yolu yok |
| Denetim izi | kısmi | `/etc/hosts` değişikliği, container silme, sertifika yenileme — ayrı, yapılandırılmış, döndürülmeyen audit log |
| Üçüncü taraf lisans bildirimi | yok | About kutusunda / NOTICE dosyasında bağımlılık lisansları — MIT dağıtım yükümlülüğü |
| Erişilebilirlik beyanı | yok | VPAT / EN 301 549 |
| Gizlilik beyanı | yok | Hangi veri nerede, ne kadar kalıyor |
| Destek / sürüm ömrü | "yalnızca en son" | LTS ya da en az N-1 backport politikası |

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
   *(Özelliğin adı bu sürümde farklı çıktı; §17.2'ye bakınız.)*

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

| İlk taslak iddiası | Gerçek | Nasıl yakalandı |
| --- | --- | --- |
| *"rustls sistem trust store'unu kullanmıyor; kurumsal MITM CA çalışmaz."* | **Yanlış.** `rustls-platform-verifier 0.7.0` graf içinde; macOS `security-framework`, Windows `windows-sys`, Linux `rustls-native-certs`. Sistem trust store kullanılıyor. Gerçek boşluk yalnızca macOS sistem *proxy'si*. | `cargo tree -e features -i reqwest` |
| *"Rust hata mesajları hiç çevrilmiyor; kullanıcı İngilizce hata okuyor."* | **Kısmen yanlış.** 12 hata kodunun **tamamı** + `UNKNOWN` çevrili ve `ErrorAlert.vue` bunu başlık olarak gösteriyor. Boşluk yalnızca spesifik mesaj ve `hint`. | `en.js:1342` `errors` bloğu okundu |
| *"Üretim kodunda 364 `unwrap/expect` var."* | **Yanlış** — o sayı test modüllerini içeriyordu. `#[cfg(test)]` öncesi bölümde toplam **7**. Bu bir kusur değil, projenin güçlü yanı. | Dosya başına `#[cfg(test)]` satır numarasına kadar sayım |
| *"MCP 34 komuta ulaşamıyor"* (README'den alınmıştı) | **README'nin kendisi yanlış.** Ölçüm: **48** komut `AppHandle` alıyor. README'nin ikinci sayısı da yanlış (§11). | `commands.rs` imza taraması |
| *"56 hata mesajı, 46 hint çevrilmiyor."* | **İkisi de yanlıştı** — ilk sayım test modüllerini içeriyor ve `format!` ile kurulan mesajları atlıyordu. Doğrusu: **113** mesaj, **33** hint. | `#[cfg(test)]` öncesi bölümde ifade tipine göre sınıflandırma |

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

| # | Madde | Ne yapıldı |
| --: | --- | --- |
| 1 | Panic hook (§4.1) | Yeni `crash.rs`: `set_hook` + `crash-<UTC>-<pid>.txt`, senkron `fs::write` ile. Mesaj `logging::redact`'ten geçiyor. Son 10 rapor tutuluyor. Hem app hem `stackvo-mcp` kuruyor. 9 test. |
| 3 | SECURITY.md (§6.1) | Advisory linki `stackvo/stackvo`'ya alındı — doğrulandı, HTTP 200. |
| 4 | README sayıları (§11) | 34 → **48**, "iki araç" → **yedi araç, adlarıyla**. Yeni `tests/readme_claims.rs` ikisini de koda karşı ölçüyor: yanlış sayı da, eksik araç adı da build'i kırıyor (kırıldığı doğrulandı). |
| 5 | Kapsam (§3.1) | `vitest --coverage` (v8) + CI'da `cargo llvm-cov`. Eşik **yok**, run summary'ye rapor. |
| 6 | Sürüm + imza uyarısı (§6.2, §6.4) | `tests/version_agreement.rs` üç dosyayı eşitliyor. `release.yml` artık macOS için de imzasız/notarize-edilmemiş durumu uyarıyor — dört senaryonun dördü de koşturularak doğrulandı. |
| 7 | `elevate` (§5.1) | Raporun "doğrusu" seçeneği: `shell(&str)` → `run(&[&str])`. Script sabit, yollar `argv` ile gidiyor, `quoted form of` kaçışlıyor. **İnterpolasyon kalmadı.** 6 test — üçü gerçek `osascript` çalıştırıp düşmanca girdiyi deniyor. |
| 8 | Sistem proxy (§5.4) | `reqwest`'e `system-proxy` + `mail.rs`'e `no_proxy()` istemci. |
| 2 | Release (§6.1) | İmza anahtarı üretildi, `pubkey` dolduruldu — `release.yml` preflight artık geçiyor. **Endpoint hâlâ 404: açık blokaj.** |

### 17.2 Raporun uygulama sırasında yanlış çıkan iki tavsiyesi

| Rapordaki iddia | Gerçek | Nasıl yakalandı |
| --- | --- | --- |
| *"`macos-system-configuration` özelliğini eklemek (tek satır)"* (§5.4) | **Özellik adı yanlış.** O ad `reqwest` 0.12'nin; bu repo 0.13.4 kullanıyor ve orada adı **`system-proxy`** (ve `default`'un parçası, `default-features = false` ile kapanıyor). Ayrıca **tek satır değil**: hyper-util'in macOS okuyucusu sistemin istisna listesini ve "Exclude simple hostnames"i okumuyor, `NO_PROXY` dışında hiçbir şey daraltmıyor. Özellik süreç geneli olduğu için `mail.rs`'in `127.0.0.1` trafiği kurumsal proxy'ye düşerdi — yani özellik, tam da açıldığı makinede mail catcher'ı bozardı. `mail::client` artık `no_proxy()` ile kuruluyor. | `reqwest-0.13.4/Cargo.toml` özellik listesi + `hyper-util`'in `matcher.rs`'i okundu |
| *"`scaffold.rs` … 791 satır, 5 test"* — sıcak ve zayıf modüller listesinde (§3.1) | **Zayıf değil: %94.09 satır kapsamı.** Test *yoğunluğu* kapsamı yanlış tahmin ediyor; §3.1'in kendi tezi bu tabloyla çürüdü. Aynı tabloda `error.rs` (%30.65), `engine.rs` (%19.65), `pty.rs` (%29.04), `watcher.rs` (%43.62) doğru çıktı. | `cargo llvm-cov --summary-only` |

### 17.3 Ölçüm artık var: ilk sayılar

Raporun §3.1'de "bilinmiyor" dediği şey artık bir sayı.

| | Satır kapsamı |
| --- | --: |
| **Rust** (toplam) | **%61.60** |
| `generator.rs` | %94.89 |
| `scaffold.rs` | %94.09 |
| `migrate.rs` | %82.46 |
| `phpini.rs` | %67.26 |
| `watcher.rs` | %43.62 |
| `db.rs` | %35.14 |
| `error.rs` | %30.65 |
| `pty.rs` | %29.04 |
| `engine.rs` | %19.65 |
| **`commands.rs`** | **%18.18** |
| **Frontend** (toplam) | **%30.70** |
| `src/lib/**` | %91.42 |
| `src/stores/**` | %78.87 |
| **`src/views/**`** | **%0** |

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

| # | Ne | Neden devredilemiyor | Bugünkü etkisi |
| --: | --- | --- | --- |
| 1 | **Güncelleme endpoint'i 404** | `tauri.conf.json` `stackvo/stackvo-tauri`'yi gösteriyor; o repo **yok** (doğrulandı: HTTP 404). Nerede yayınlanacağı bir sahiplik kararı — `stackvo/stackvo` release'leri mi, yeni bir repo mu. | İmza tarafı çözüldü, dağıtım tarafı çözülmedi: **uygulama hâlâ güncelleme alamaz.** Blokajın *ikinci* yarısı bu. |
| 2 | **`TAURI_SIGNING_PRIVATE_KEY` secret'ı** | Özel anahtar üretildi ve `~/.tauri/stackvo.key`'de duruyor (mod 600); public yarısı `tauri.conf.json`'a girdi. Özel yarı **repoya girmedi ve girmemeli** — GitHub repository secret'ı olarak eklenmesi gerekiyor. Parolasız üretildi; parolalı istenirse çift yeniden üretilmeli. | `release.yml` preflight'ının pubkey kontrolü artık geçiyor, secret kontrolü hâlâ bloke ediyor — doğru davranış. |
| 3 | **Apple / Windows imzalama secret'ları** | `APPLE_CERTIFICATE`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`, `WINDOWS_CERTIFICATE`. Hepsi ücretli geliştirici hesaplarına bağlı. | §6.4'ten sonra artık **sessiz değil**: eksikse release log'unda uyarı çıkıyor. Ama hâlâ eksikler. |
| 4 | **Kapsam eşiği** | Sayılar artık var (§17.3). %61.60'ı mı yoksa daha düşük bir tabanı mı kilitleyeceği mühendislik değil, politika kararı. | Ölçüm var, kapı yok. |

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
clone), **hiç testi yoktu.** Yazılmamış değil; *yazılamazdı*.

**Dilim 1.** Yeni `progress.rs` — içinde tek bir `use tauri::` yok:

| | |
| --- | --- |
| `trait ProgressSink` | `fn event(&self, name: &str, payload: Value)`. `dyn`-uyumlu olması için jenerik değil; payload `Value`, çünkü zaten webview'e JSON olarak gidiyordu. |
| `Null` | Pencere yok, olaylar düşer. `stackvo-mcp` artık `Sink::Headless` yerine bunu kullanıyor — MCP yolu artık hiçbir Tauri tipi adlandırmıyor. |
| `Recording` | Var olmayan implementasyon. Olayları sırasıyla tutar; `names()`, `named()`, `last()`. |

`events::Sink` trait'i implement ediyor, `run_operation` artık
`&dyn ProgressSink` alıyor — masaüstü çağrı yerleri değişmedi (`&Sink`
otomatik `&dyn`'e dönüşüyor), webview aynı JSON'u alıyor.

Sonra `run_operation`'ın dört dalı da test edildi: başarı (satır başına progress
+ tek terminal olay), sıfır-olmayan çıkış (hem `Err` **hem** başarısız terminal
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
`"dark"`'ı ya da bir diziyi *geçerli JSON* olarak kabul eder ve eski kod onu
öylece döndürüyordu. Sonrasında her `prefs_set` çağrısı `as_object_mut()`'tan
`None` alıyor, shallow merge hiçbir şey yapmıyor, ve aynı skaler geri
yazılıyordu. Yani: **kullanıcı ayarları değiştiriyor, hiçbiri kaydedilmiyor,
diskte geçerli bir dosya var ve hiçbir yerde hata yok.**

Yapılan: `schemaVersion: 1` (ileride yeniden adlandırılacak bir anahtar için tek
tutamak), nesne olmayan JSON de bozuk sayılıyor, ve bozuk dosya
**kopyalanmıyor — taşınıyor** (`preferences.corrupt-<UTC>.json`). Taşımak
kasıtlı: kopyalasaydık bozuk dosya orada durduğu sürece her açılışta yeni bir
yedek üretilirdi. Yalnızca *bozuk* JSON'da çalıştığı için güvenli — daha yeni bir
sürümün bilinmeyen anahtarlar taşıyan dosyası hâlâ geçerli bir nesnedir, okunur
ve karantinaya alınmaz (bu da ayrıca test edildi).

### 18.3 §14.13 — SBOM, provenance, checksum

Üçü de eklendi ve **yerelde çalıştırılarak** doğrulandı:

- **SBOM, iki dosya.** `cargo cyclonedx` (380 bileşen) + `npm sbom` (16, prod).
  Tek dilli bir SBOM, Tauri uygulamasının yarısını atlayan bir belgedir.
- **Build provenance** — `actions/attest-build-provenance`, artı iş için
  gereken `id-token: write` ve `attestations: write` izinleri.
- **SHA-256 listesi** — `latest.json` imzalı olduğu için *updater* kanıtlayabiliyordu;
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

| | Başlangıç | Şimdi |
| --- | --: | --: |
| **Rust toplam** | %61.60 | **%63.12** |
| `runner.rs` | (`run_operation` test edilemez) | **%98.17** |
| `commands.rs` | %18.18 | **%23.97** |
| `progress.rs` | — | %98.06 |

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
> cannot be trusted either`

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

| Ne | Neydi | Ne yapıldı |
| --- | --- | --- |
| `npm run lint` exit 1 | Dört dump dosyası Prettier'dan geçmiyordu — yani "Lint the front end" adımı main'de kırmızıydı. | `prettier --write`. Dördü de saf biçimlendirme; diff'i satır satır kontrol edildi, anlamsal değişiklik yok. |
| ESLint `coverage/` dizinini tarıyordu | **Bu turun kendi regresyonu.** §17.5'te açılan kapsam raporu, v8 reporter'ın kendi HTML paketini üretiyor; ESLint onun `eslint-disable` yorumlarını "kullanılmayan direktif" diye raporluyordu. | `eslint.config.js` ignore listesine `coverage/**`. |

### 19.2 Hermetik olmayan test

`preflight::tests::a_fresh_install_asks_for_the_two_core_names_and_nothing_else`
düşüyordu ve HEAD'de de düşüyordu — o yüzden "pre-existing" diye kaydedilmişti.
Sebebi kaydedilmemişti, ve sebep kodda değildi.

Test `missing_hosts_by_owner`'ı çağırıyor; o zincir **gerçek Docker daemon'ına**
ve **gerçek `/etc/hosts`'a** ulaşıyor. Testin kendi yorumu şöyle diyordu:

> *Nothing here starts Docker, and that is the point: `stackvo_containers`
> fails, nothing is running…*

Doğru, ve aynı şey değil. Docker'ı *başlatmamak* ile hiçbir şeyin *çalışmıyor
olması* farklı iddialar. CI runner'ında ikisi çakışıyor, o yüzden test yeşildi.
Stack'i fiilen çalıştıran bir geliştirici makinesinde phpMyAdmin ve RabbitMQ
**çalışıyor**, kod onları doğru şekilde listeliyor, ve test kodda olmayan bir
hatayı bildiriyordu. **Yani: bakımcının kendi makinesinde koşamayan bir test.**

`service_domains`'in iki ortam okuması (çalışan container'lar, `/etc/hosts`)
argümana çevrildi. Kural artık *belirtilen* bir dünyaya karşı doğrulanıyor:
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

*(Bu arada bir beklentim yanlış çıktı: bilinmeyen servis `InvalidInput` değil
`NotFound` dönüyor. Kod haklı — ad iyi biçimli, sadece hiçbir şeyi
adlandırmıyor — ve ikisi kullanıcıya farklı çevrilmiş başlık olarak ulaştığı için
hangisi olduğu davranıştır. Test gerçeğe uyduruldu.)*

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
*hiçbir şey kontrol etmeden* geçerdi. Hiç koşmamasından kötü olurdu: vermediği bir
garantiyi veriyormuş gibi görünen yeşil bir suite. Üstelik bu uygulamanın en çok
ihtiyaç duyduğu kural o — `appearance.js` temayı işletim sisteminin vurgu
renginden türetiyor, yani palet sabit değil ve elle bir kez denetlenemez. Gerçek
bir tarayıcı gerekiyor: §14.12.

### 19.6 Sayılar

| | Tur 1 | Tur 2 | Şimdi |
| --- | --: | --: | --: |
| **Rust toplam** | %61.60 | %63.12 | **%63.34** |
| `commands.rs` | %18.18 | %23.97 | **%25.46** |
| **Frontend toplam** | %30.70 | %30.70 | **%31.44** |
| Rust testleri | 448 | 469 | **472** |
| Frontend testleri | 160 | 160 | **182** |

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

*(Aradaki fark muhtemelen §15'te kaydedilen `format!` sorununun aynısı: ilk
sayım çok satırlı literal'leri ve `.to_string()` ile yazılmış olanları
atlıyordu. Aynı hatanın hem mesaj hem hint sayımında tekrarlanması, "ifade
tipine göre sınıflandırma"nın tek seferlik bir düzeltme değil, sürekli bir
yöntem sorunu olduğunu gösteriyor.)*

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

| | Önce | Sonra |
| --- | --: | --: |
| Çevrilmeyen hint | 60 | **3** (çalışma anında kurulanlar) |
| Türkçe hint çevirisi | 0 | **56** |
| Rust testleri | 472 | **475** |
| Frontend testleri | 182 | **186** |
| Rust kapsam | %63.34 | **%63.47** |

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
job harici bir repo checkout'una bağlı**. Rapor bunu §2.1'de kusur değil *risk*
diye işaretlemişti ve haklıydı: `stackvo/stackvo` private olduğu, adı
değiştiği ya da rate-limit'e takıldığı gün sözleşme kapısı kaybolur ve kimse
fark etmez. Bu yarısı ağ, checkout ve Node istemiyor — o job koşamadığında da
koşuyor.

Yakaladığı hata sınıfı somut: `commands.rs`'e yazılıp `lib.rs`'e eklenmeyen bir
komut **derlenir ve sessizce geçer**; çalışma anında "command not found" olarak,
kimsenin geliştirme sırasında açmadığı bir ekranda ortaya çıkar.

### 21.6 Sayılar

| | Önce | Sonra |
| --- | --: | --: |
| Rust testleri | 475 | **481** |
| `diagnostics.rs` kapsam | — | **%94.58** |
| Rust toplam kapsam | %63.47 | **%64.34** |
| IPC komutu | 143 | **144** |

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
kendi tezinin ("*'muhtemelen aynı' shipping için bir standart değil*") tam
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

| Yer | Komut |
| --- | --- |
| `LogView.vue` | `app_logs_all`, `app_logs` |
| `DumpView.vue` | `debug_bridge_overview` |
| `Projects.vue` | `project_adoptable` |

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
  *sayfa* taramalarında, kaynağı adlandırılarak kapatıldı; bileşen taramalarında
  kural açık kalıyor — `aria-progressbar-name`'i her yerde kapatmak, bu dosyayı
  haklı çıkaran bulguyu çöpe atmak olurdu.

### 22.6 Sayılar

| | Önce | Sonra |
| --- | --: | --: |
| **Frontend toplam kapsam** | %31.44 | **%50.71** |
| **`src/views/`** | **%0** | **%26.38** |
| `About` / `Dumps` / `Logs` | %0 | **%100** |
| `Services` | %0 | **%94.38** |
| `Dashboard` | %0 | **%85.42** |
| `Projects` | %0 | **%84.25** |
| `Mail` | %0 | **%78.78** |
| Frontend testleri | 186 | **228** |
| Rust testleri | 481 | 481 |

Geriye `Settings.vue` ve `ProjectDetail.vue` kaldı — ikisi de **%0**, ve ikisi de
§14.16'nın (bölme) konusu. `src/views/`'in %26'da kalmasının tek sebebi onlar.

### 22.7 Açık kalanlar

- **§14.12 E2E** — bir Linux runner gerekiyor. Klavye-only gezinme, focus
  tuzakları ve `color-contrast` (§19.5) hâlâ yalnızca orada ölçülebilir.
- **§14.10 tauri-specta** — §22.3'ün dördüncü kez bulduğu hatanın tek kalıcı
  çözümü.
- **§14.16** — `Settings.vue` ve `ProjectDetail.vue`.

