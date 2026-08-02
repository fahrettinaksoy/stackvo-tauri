# Platform matrisi — masaüstü ve web

StackVo-Tauri'nin kullandığı her Docker ve host yeteneğinin Windows, Linux,
macOS ve olası bir web sürümünde ne yapacağı. Koddan çıkarıldı, tahminle değil;
her satırın dayandığı dosya ve fonksiyon belirtildi. Neyin ölçüldüğü ve neyin
yalnızca okunduğu §7'de ayrı ayrı yazılı.

**Kapsam sorusu:** "Tüm işlemleri API'ya çekersek Docker yönetimi sorunsuz
çalışacak mı?" Kısa cevap: **evet — sunucu host'ta ve GUI oturumu içinde
çalıştığı sürece.** Ayrım Docker'da değil, sunucunun *nerede* çalıştığında.

## 0. Varsayım — bu her şeyi değiştirir

Bu rapor şu modeli varsayar:

> Web arayüzüne ulaşabilmek için **masaüstü uygulaması kurulmuş olacak.**
> HTTP sunucusu ya masaüstü uygulamasının kendisi, ya da onunla aynı host'ta,
> aynı kullanıcı oturumunda çalışan kardeş bir ikilidir.

Bu varsayım küçük görünüyor ama sonucun yarısını belirliyor, çünkü:

* **Sertifika ve hosts işi zaten çözülmüş olur.** İlk açılış ekranı masaüstünde
  çalışır; CA güvenilir, `/etc/hosts` yazılmış, sertifika üretilmiş olur.
  Tarayıcı bunları tetiklemek zorunda kalmaz — *çoktan olmuşlardır.*
* **Native diyaloglar çalışmaya devam eder.** Klasör seçici, editörde açma,
  terminal açma — bunların hepsi host'ta, GUI oturumu içinde çalışır. Tarayıcı
  yalnızca tetikleyicidir; pencere kullanıcının ekranında açılır.

Bu varsayım **düşerse** — sunucu başsız (headless) bir makinede çalışırsa, ya da
arayüz *başka bir cihazdan* açılırsa — 4c'deki her şey gerçekten kırılır.
Ayrıntı için §4e.

---

## 1. Ölçüm

| | Sayı |
|---|---|
| Toplam IPC komutu | 142 |
| Frontend kaynak dosyası | 47 |
| Bunlardan `@tauri-apps` kullanan | 12 |
| **Veri katmanının geçtiği fonksiyon** | **1** (`src/lib/ipc.js` → `call()`) |
| Docker'a bollard (API) ile giden komut | 12 |
| Docker'a `docker compose` (CLI) ile giden komut | 16 |
| Host dosya sistemine dokunan komut | 17 |
| Ayrıcalık (parola) gerektiren komut | 9 |
| Masaüstüne özgü komut | 15 (bunların yalnızca **4**'ü web'de gerçekten anlamsız — §4c) |

Veri yolunun tek fonksiyondan geçmesi, bu raporun en önemli tek bulgusu:

```js
// src/lib/ipc.js
export async function call(command, args = {}) {
  return await invoke(command, args);   // ← web sürümünde fetch olur
}
```

136 sarmalayıcının hepsi buradan geçiyor. Gövdesi değişirse kalan 46 dosya
değişmez.

---

## 2. Docker erişimi — iki ayrı yol

Uygulama Docker'a **iki farklı şekilde** bağlanıyor ve ikisinin platform
davranışı farklı.

### 2a. Bollard — Docker Engine API (soket/pipe)

`src-tauri/src/engine.rs`, tek modül. Kullanılan API çağrıları:

| Çağrı | Ne için |
|---|---|
| `version` | Motor ayakta mı, sürüm ne |
| `list_containers` ×3 | Konteyner envanteri, port sahipleri, disk dağılımı |
| `inspect_container` | Detay paneli |
| `stats` | Canlı CPU/bellek |
| `logs` | Konteyner log akışı |
| `events` | Konteyner durum değişikliklerini dinleme (poll değil, push) |
| `df` | İmaj/volume envanteri |
| `list_images` | İmaj listesi |
| `create_network` | `stackvo-net` oluşturma |
| `prune_images` / `prune_volumes` | Temizlik |

**Soket çözümü** (`engine.rs:141` `resolve_endpoint`), Docker CLI'ın sırasıyla:

1. `DOCKER_HOST`
2. Seçili `docker context` (`~/.docker/contexts/meta/*/meta.json`)
3. Bilinen yollar

| Platform | Bilinen yollar |
|---|---|
| macOS / Linux | `~/.docker/run/docker.sock` (Docker Desktop), `~/.colima/default/docker.sock`, `~/.orbstack/run/docker.sock`, `/var/run/docker.sock` |
| Windows | `\\.\pipe\docker_engine` (named pipe) |

Windows'ta soket değil **named pipe** kullanılıyor; `paths.rs` `is_named_pipe`
iki yazımı da tanıyor (`npipe:////./pipe/...` ve `\\.\pipe\...`).

### 2b. Docker CLI — `docker compose`

Yığını ayağa kaldırmak, indirmek, derlemek ve `docker exec` çalıştırmak
bollard ile değil, **`docker` ikilisi çağrılarak** yapılıyor
(`src-tauri/src/runner.rs`). Sebep: `--profile`, çok dosyalı `-f` birleştirme ve
`--build` gibi davranışlar Compose'un kendi mantığı; API'da karşılığı yok.

Kullanılan alt komutlar: `up`, `down`, `build`, `start`, `stop`, `restart`,
`logs`, `exec`, `config`, `rm`.

Ek olarak `docker exec` şuralarda doğrudan kullanılıyor: veritabanı dökümü
(`db.rs`), PHP eklenti denetimi (`phpini.rs`), hızlı komutlar (`quickcmd.rs`).

---

## 3. Docker yetenekleri — platform matrisi

| Yetenek | Kaynak | Windows | Linux | macOS | Web (yerel sunucu) |
|---|---|---|---|---|---|
| Motor durumu / sürüm | bollard `version` | ✅ | ✅ | ✅ | ✅ |
| Konteyner listesi / detay | bollard | ✅ | ✅ | ✅ | ✅ |
| Canlı istatistik (CPU/RAM) | bollard `stats` | ✅ | ✅ | ✅ | ✅ akış SSE/WS olur |
| Konteyner logları | bollard `logs` | ✅ | ✅ | ✅ | ✅ akış SSE/WS olur |
| Konteyner olayları (push) | bollard `events` | ✅ | ✅ | ✅ | ✅ akış SSE/WS olur |
| Disk kullanımı | bollard `df` | ✅ | ✅ | ✅ | ✅ |
| Ağ oluşturma | bollard `create_network` | ✅ | ✅ | ✅ | ✅ |
| Prune (imaj/volume) | bollard | ✅ | ✅ | ✅ | ✅ |
| `compose up/down/build` | docker CLI | ✅ | ✅ | ✅ | ✅ |
| `docker exec` (döküm, denetim) | docker CLI | ✅ | ✅ | ✅ | ✅ |
| **Motoru başlatma** | `engine.rs:236` | `cmd /C start "Docker Desktop.exe"` | `systemctl --user start docker-desktop` | `open -a Docker` | ✅ sunucu host'ta olduğu için aynı komut |
| **Bind mount yolları** | `paths.rs` | ⚠️ `C:\x` → `/c/x` çevirisi | ✅ | ✅ | ✅ sunucu host'ta ise |

**Sonuç:** Docker'ın kendisi dört ortamda da sorunsuz. Bollard bir HTTP
istemcisi; sunucu host'ta çalıştığı sürece web sürümünde hiçbir fark yok.
Akışlar (log, stats, events) IPC olayı yerine SSE veya WebSocket'e taşınır — bu
bir taşıyıcı değişikliği, yetenek kaybı değil.

---

## 4. Docker dışı — asıl ayrım burada

Bu uygulamanın masaüstü olma sebebi Docker değil, Docker'ın **etrafındaki**
işler. Web sürümünde ayrışan kısımlar bunlar.

### 4a. Ayrıcalık gerektirenler (parola)

| İş | Kaynak | Windows | Linux | macOS | Web |
|---|---|---|---|---|---|
| `/etc/hosts` yazma | `hosts.rs` + `elevate.rs` | ✅ PowerShell `-Verb RunAs` (UAC) | ✅ `pkexec` (polkit) | ✅ `osascript … with administrator privileges` | ⚠️ sunucu host'ta ve ayrıcalıklıysa; tarayıcıdan tetiklenir, sunucuda çalışır |
| CA'yı güven deposuna ekleme | `certs.rs` | ❓ mkcert'e bırakılmış (`certutil` kullanıcı deposu) — **denenmedi** | ❓ mkcert + `pkexec` — **denenmedi** | ⚠️ **terminal açılarak** (`cert_trust_in_terminal`) — ölçüldü | ✅ **gerekmez** — masaüstü kurulumunda çoktan yapılmış olur |

macOS'ta CA güveni ölçülerek şu üç yolun da çalışmadığı bulundu: `sudo`
terminalsiz sonsuza kadar bekliyor, AppleScript'le root doğrudan reddediliyor
(`SecTrustSettingsSetTrustSettings: authorization denied`), kullanıcı alanına
yazma 0 dönüp hiçbir şey yapmıyor. Çözüm: uygulama kullanıcının terminalini
açıyor. **Bir web sürümü terminal açamaz** — bu iş kullanıcıya komut olarak
gösterilir.

### 4b. Host dosya sistemi

| İş | Kaynak | Web'de |
|---|---|---|
| Workspace/proje dizini okuma-yazma | `workspace.rs`, `scaffold.rs` (791 satır) | ✅ sunucu host'ta ise |
| Compose/config üretimi | `generator.rs`, `template.rs` | ✅ |
| `.env` okuma-yazma | `config.rs`, `env_writer.rs` | ✅ |
| Şablon override | `skeleton.rs` | ✅ |
| Uygulama logları (dosyadan) | `applog.rs` | ✅ |
| **Dosya değişikliği izleme** | `watcher.rs` (`notify` crate) | ⚠️ sunucuda çalışır, tarayıcıya push gerekir |
| **Klasör seçici** | `workspace_pick` (native dialog) | ✅ diyalog **host'ta** açılır, tarayıcı yalnızca tetikler |

Klasör seçici ilk bakışta web'in en sert sınırı gibi görünür — tarayıcı güvenlik
modeli bir dizinin mutlak yolunu JavaScript'e vermez. Ama burada yol tarayıcıya
hiç gitmiyor: istek sunucuya gider, sunucu host'ta native diyaloğu açar,
kullanıcı kendi ekranındaki pencereden seçer, yol **sunucuda** kalır. Tarayıcı
sadece düğmeye basar.

Bu yalnızca sunucu bir GUI oturumu içinde çalışırken geçerli — §4e.

### 4c. Native kabuk — çoğu çalışır, birkaçı anlamsız

İlk taslakta buraya "web'de karşılığı yok" diye 15 komut yazmıştım. Yanlıştı.
Sunucu host'ta ve GUI oturumu içinde çalışıyorsa, bu işlerin çoğu **çalışır** —
tarayıcı yalnızca uzaktan kumandadır ve pencere kullanıcının kendi ekranında
açılır.

| Yetenek | Kaynak | Sunucu host'ta + GUI oturumunda |
|---|---|---|
| Klasör seçici | `workspace_pick` | ✅ diyalog host'ta açılır |
| Editörde açma | `open_in_editor` | ✅ VS Code host'ta açılır |
| Klasörü Finder/Explorer'da açma | `open_folder` | ✅ |
| Harici terminal açma | `terminal_open_external` | ✅ |
| CA'ya güven (terminal) | `cert_trust_in_terminal` | ✅ ama zaten gerekmez |
| Tarayıcıda açma | `open_in_browser` | ✅ (tarayıcı kendisi de açabilir) |
| Tercihler | `prefs_get` / `prefs_set` | ✅ host'ta dosya |
| Kurulu uygulamalar | `apps_available` | ✅ |
| Sistem vurgu rengi | `system_accent` | ✅ host'tan okunur |
| Gömülü terminal (PTY) | `pty.rs` — `portable-pty` | ⚠️ sunucu tarafı PTY + xterm.js; taşıyıcı işi, yetenek kaybı değil |
| Masaüstü bildirimleri | `plugin-notification` | ⚠️ Web Notifications API |
| **Sistem tepsisi** | `tray.rs` | ❌ tarayıcı sekmesinin tepsisi olmaz (masaüstü penceresininki çalışmaya devam eder) |
| **Native menü çubuğu** | `menu.rs` | ❌ aynı sebep |
| **Otomatik başlatma** | `plugin-autostart` | ❌ masaüstü uygulamasının özelliği |
| **Otomatik güncelleme** | `plugin-updater` | ❌ web zaten hep güncel |
| **Pencere boyutu/konumu** | `lib.rs` setup | ❌ tarayıcı penceresi kullanıcının |

Gerçekten karşılığı olmayan **4 kalem**: tepsi, native menü, otomatik başlatma,
otomatik güncelleme. Hepsi *masaüstü penceresinin kendisiyle* ilgili — yani web
arayüzünün eksiği değil, sadece kapsamı dışında. Masaüstü uygulaması kuruluysa
onlar da çalışmaya devam eder.

### 4e. Varsayım düştüğünde

§0'daki varsayım iki şekilde bozulur ve ikisi de tabloyu değiştirir.

**Sunucu başsız çalışırsa** (SSH ile bağlanılan bir makine, GUI oturumu yok):
klasör seçici, editör, terminal ve CA güven adımı kırılır — açacak bir ekran
yoktur. Docker ve dosya sistemi işleri etkilenmez. Bu senaryoda arayüz "yolu
elle yaz" moduna düşmeli ve bu işleri gizlemelidir.

**Arayüz başka bir cihazdan açılırsa** (telefon, ağdaki başka bir dizüstü):

| | Durum |
|---|---|
| StackVo arayüzü | ✅ açılır |
| Docker yönetimi | ✅ çalışır (sunucu host'ta) |
| Native diyaloglar | ⚠️ host'un ekranında açılır — uzaktaki kullanıcı göremez |
| `stackvo.loc` ve proje siteleri | ❌ o cihazda `/etc/hosts` kaydı yok, CA güvenilmiyor |

Yani ikinci cihazdan yığını **yönetebilirsiniz** ama projelerin sitelerini
**açamazsınız** — o cihazın kendi hosts dosyası ve kendi güven deposu gerekir ve
uygulama uzaktan onlara dokunamaz. Bu, mimari bir sınır değil, işletim
sisteminin sınırı.

### 4d. Ölçülen ve ölçülmeyen

Bu rapor koddan çıkarıldı. Davranışın **gerçekten çalıştırılarak** doğrulandığı
tek platform macOS — bu makinede. Windows ve Linux sütunları koddaki
`#[cfg(target_os = …)]` dallarının okunmasından geliyor ve o dallar bu oturumda
çalıştırılmadı.

Bunu ayrıca söylüyorum çünkü bu oturumda macOS'ta üç ayrı sertifika-güven
yöntemi "kod doğru görünüyor" diye kabul edildi ve üçü de çalıştırıldığında
yanlış çıktı. Aynı riskin Windows ve Linux'ta da olduğunu varsaymak doğru olur:

| Satır | Durum |
|---|---|
| Docker API (bollard) | Platformdan bağımsız HTTP istemcisi — risk düşük |
| Docker CLI (`compose`) | Aynı ikili, aynı bayraklar — risk düşük |
| Soket/pipe çözümü | Windows named pipe kodu **testlerle** kapsanıyor (`paths.rs`), çalıştırılarak değil |
| Bind mount yol çevirisi | Aynı — `to_docker_mount` testleri her platformda koşuyor |
| `/etc/hosts` yükseltmesi | UAC ve polkit dalları **hiç çalıştırılmadı** |
| CA güveni | Yalnızca macOS ölçüldü; diğer ikisi mkcert'e bırakılmış |
| PTY | `portable-pty` Windows'ta ConPTY kullanır — **denenmedi** |

---

## 5. Web sürümünün mimarisi

### 5a. API Rust'ta yazılır — yeni dil yok, yeniden yazım yok

Bir soru olarak geldiği için ayrıca cevaplıyorum: **web arayüzünün arkasındaki
API Rust olur.** Node.js ya da başka bir dilde ikinci bir uygulama yazmak, bu
projede 32.515 satırlık çekirdeği baştan yazmak demektir — Docker istemcisi,
compose üreteci, şablon motoru, sertifika yönetimi, hosts ayrıştırıcısı, PTY,
doctor, migrate. Ve ardından iki uygulamanın sonsuza kadar aynı şeyi söylemesini
sağlamak.

Bu projede o yaranın izi zaten var: `contracts/ipc.json`, **iki ayrı uygulama
birbiriyle çeliştiği için** yazılmış bir sözleşme dosyası, ve emekli edilen
konteynerli web arayüzü Node/Express'ti.

Gerekmiyor da, çünkü çekirdek **zaten kütüphane**:

```toml
[lib]
name = "stackvo_desktop_lib"
crate-type = ["staticlib", "cdylib", "rlib"]
```

Ve bu kütüphane Tauri olmadan çalıştırıldı — bu raporu üretirken dört kez.
`examples/preflight_report.rs`, `metrics_probe.rs`, `cpu_truth.rs`,
`diagnose.rs` pencere açmadan `preflight::`, `certs::`, `hosts::`,
`commands::write_generated()` çağırıyor. Yani "Tauri'siz çalışır" bir tahmin
değil, tekrarlanabilir bir olgu.

### 5b. Şekli

```
src-tauri/
├── src/lib.rs          32.515 satır — hiç değişmez
├── src/main.rs         Tauri ikilisi (bugünkü)
└── src/bin/serve.rs    YENİ — HTTP ikilisi, host üzerinde

                 tek Vue arayüzü
                 ayrım noktası: src/lib/ipc.js → call()
```

`serve.rs` kabaca:

```rust
use stackvo_desktop_lib::commands;

async fn projects(State(ctx): State<Ctx>) -> Json<Vec<Project>> {
    Json(commands::list_projects(&ctx.root).await?)
}
```

Gövdeler zaten var — Tauri komutlarının altındaki saf fonksiyonlar. 141
komutun 108'i `State<'_, AppState>` alıyor; bunlar ince sarmalayıcılar ve
altlarındaki mantık örneklerin çağırdığı fonksiyonlarda.

Frontend tarafında değişen tek şey:

```js
export async function call(command, args = {}) {
  return TAURI ? invoke(command, args)
               : post(`/api/${command}`, args);
}
```

**Kritik tasarım kararı:** sunucu **host üzerinde** çalışır, konteynerde değil.
Sprint 19'da emekli edilen konteynerli web arayüzü tam da burada battı —
konteynerin içinden host'un Docker soketini, `/etc/hosts`'unu, mkcert'ini ve
editörünü göremiyordu.

### 5c. Yapılacak iş

Yönlendirme mekanik. Asıl iş üç yerde.

| Adım | Büyüklük | Not |
|---|---|---|
| `call()` gövdesini taşıyıcıya göre ayır | ~20 satır, 1 dosya | `invoke` frontend'de yalnızca burada geçiyor |
| 142 komut için HTTP yönlendirici | mekanik | `contracts/ipc.json` argüman ve dönüşleri zaten tarif ediyor |
| **Akışlar** (log, stats, events) | orta | Tauri olayından SSE/WebSocket'e; taşıyıcı değişikliği ama yeniden yazım |
| **Kimlik doğrulama** | küçük ama **atlanamaz** | §5d |
| Yetenek katmanı (4 komut için arayüz gizleme) | küçük | 12 dosyaya dokunur |
| Başsız mod için dizin gezgini | orta | yalnızca §4e senaryosu için |

### 5d. Kimlik doğrulama — atlanamaz olan

Yerel bir HTTP sunucusu "sadece localhost" olduğu için güvenli **değildir.**
Kullanıcının açtığı herhangi bir web sitesi, tarayıcıdan `localhost`'a istek
atabilir. Bu API'nin yapabildikleri:

* konteyner başlatmak ve durdurmak
* `/etc/hosts` yazmak (ayrıcalıkla)
* proje silmek — `remove_dir_all` dahil
* `docker exec` ile konteyner içinde komut çalıştırmak
* veritabanı dökümü almak

Bir token ya da eşdeğeri **ilk günden** olmalı, sonradan eklenecek bir madde
olarak değil. İş listesindeki en küçük kalem ama atlanması en kolay ve sonucu
en ağır olanı.

---

## 6. Özet cevap

**"Docker yönetimi sorunsuz çalışacak mı?"**

| | Cevap |
|---|---|
| Docker API işlemleri (envanter, istatistik, log, olay, ağ, prune) | ✅ dört ortamda da aynı |
| `docker compose` işlemleri (up/down/build/exec) | ✅ dört ortamda da aynı |
| Host dosya sistemi (üretim, .env, projeler, loglar) | ✅ sunucu host'ta çalıştığı sürece |
| Sertifika ve `/etc/hosts` | ✅ web'de **gerekmez** — masaüstü kurulumunda çözülmüş olur |
| Native kabuk (terminal, editör, seçici) | ✅ sunucu GUI oturumundaysa host'ta açılır |
| Tepsi, native menü, otomatik başlatma/güncelleme | ❌ tarayıcı sekmesinin kapsamı dışında |

Bugünkü 142 komutun **~138'i** web'de çalışır. Geriye kalan 4'ü — tepsi, native
menü, otomatik başlatma, otomatik güncelleme — *masaüstü penceresinin kendisine*
ait şeyler; web arayüzünün eksiği değil, kapsamı dışı.

İlk taslakta bu sayı 127 idi. Farkı yaratan, §0'daki varsayım: web'e ulaşmak
için masaüstü kurulu olacaksa, sunucu host'ta ve bir GUI oturumu içindedir —
native diyaloglar da terminal de çalışır, tarayıcı yalnızca düğmeye basar. Ve
sertifika işi, bu oturumun yarısını yiyen konu, web'in problemi olmaktan çıkar:
arayüze ulaşan kişinin makinesinde çoktan çözülmüştür.

Bu tabloyu mümkün kılan iki karar:

1. **Veri yolu tek bir `call()` fonksiyonundan geçiyor.** `invoke` kelimesi
   47 dosyalık arayüzde tam olarak bir yerde geçiyor. Bu karar alınmamış olsaydı
   aynı iş 136 çağrı yerine yayılmış olurdu.
2. **Çekirdek Tauri'ye değil, Tauri çekirdeğe bağlı.** `lib.rs` bir kütüphane;
   `main.rs` onu kullanan ince bir ikili. İkinci bir ikili eklemek mimari bir
   değişiklik değil, mevcut yapının zaten desteklediği bir şey.

---

## 7. Bu raporun statüsü

**Bu bir analiz, ölçüm değil.** Her satır koddan çıkarıldı ve dosya/fonksiyon
adıyla dayanaklandırıldı; ama henüz hiçbir web sürümü yazılmadı ve
çalıştırılmadı.

Bu ayrımı özellikle yazıyorum çünkü bu raporun üretildiği oturumda, macOS'ta
sertifika güveni için **üç ayrı yöntem** "kod doğru görünüyor" diye kabul edildi
ve üçü de çalıştırıldığında yanlış çıktı — biri sonsuza kadar bekledi, biri
reddedildi, biri sıfır dönüp hiçbir şey yapmadı. Doğru cevap ancak
`security verify-cert` çalıştırılınca ortaya çıktı.

Bu raporda aynı riski taşıyan satırlar:

| Satır | Risk |
|---|---|
| Windows ve Linux sütunlarının tamamı | Bu oturumda o dallar hiç çalıştırılmadı |
| Akışların SSE/WS'e taşınması | "Taşıyıcı değişikliği" diye yazıldı; yazılmadan doğrulanmaz |
| Native diyalogların tarayıcıdan tetiklenmesi | Mantıken doğru; denenmedi |
| PTY'nin sunucu tarafında xterm.js ile çalışması | Yaygın bir desen, ama bu kod tabanında denenmedi |

Risk düşük olan tek grup, Docker'ın kendisi: bollard platformdan bağımsız bir
HTTP istemcisi ve `docker compose` her yerde aynı ikili.
