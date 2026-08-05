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

### Şimdi (gün–hafta)

1. **Panic hook + crash dosyası** (§4.1) — ~15 satır. `panic = "abort"` ile
   bugün her çökme izsiz. **Tek en yüksek getirili düzeltme.**
2. **Release blokajlarını kaldır** (§6.1) — anahtar çifti üret **ve** endpoint'i
   ayağa kaldır. İkisi ayrı işler; bugün ikisi de eksik.
3. **SECURITY.md'deki 404 advisory linkini düzelt** (§6.1) — güvenlik bildirim
   yolunun kırık olması tek satırlık ama ciddi bir kusur.
4. **README'deki iki yanlış sayıyı düzelt** (§11) — özellikle
   `--allow-writes`'ın 7 aracı açtığı; bu bir güvenlik dokümanı satırı.
5. **Kapsam ölçümünü aç** (§3.1) — `cargo llvm-cov` + `vitest --coverage`,
   eşiksiz başla, sadece **gör**.
6. **Sürüm numarası eşitlik testi** (§6.2) ve **macOS imzasız-build uyarısı**
   (§6.4) — ikisi de birkaç satır.
7. **`elevate` quoting'i düzelt** (§5.1) — `osascript … on run argv`.
8. **`macos-system-configuration` özelliğini ekle** (§5.4) — tek satır.

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
