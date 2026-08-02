# Rekabet boşlukları — ikinci ölçüm, Ağustos 2026

`docs/competitive-analysis.md` kapandı: içindeki her P0–P3 maddesi ya teslim
edildi ya da kararla kapatıldı. Bu doküman, o 23 sprint yapılırken yerinde
durmayan bir sahaya karşı alınan **bir sonraki** ölçüm.

**Yöntem.** On ürün 2026-08-01'de okundu — açılış sayfaları ve dokümantasyon
indeksleri, bir özellik tablosuna güvenmek yerine navigasyon takip edilerek.
Rakip sütunları satıcının *iddia ettiğini* kaydeder; boş hücre *iddia
edilmemiş* demektir, *yok olduğu kanıtlanmış* demek değil.

**StackVo sütunu farklı.** Her hücre bu repoya karşı doğrulandı ve bir dosya
referansı taşıyor. Bir iddia ile bir dosya çelişiyorsa dosya kazanır. Bu
dokümanın ilk taslağındaki dört iddia o kontrolden geçemedi ve silindi:
StackVo'nun `.nvmrc` okuyamadığı (okuyor — `detect.rs:450`), node projelerinde
bind mount olmadığı (Sprint 8'den beri var), PHP ve node dışında runtime
olmadığı (altı tane var) ve MCP sunucusunun salt okunur olduğu (Sprint 16'dan
beri yazma yüzeyi var).

**Listeden önce yapısal bir not.** Önceki analiz dokuz ürünü karşılaştırdı.
**DDEV bunların arasında yoktu ve sahadaki mimari olarak en yakın rakip o** —
Docker tabanlı, proje başına stack, paylaşılan Traefik router, mkcert HTTPS,
repoya işlenen config. Aşağıdaki boşlukların kabaca üçte biri tek başına bu
atlamadan geliyor. Ayrıca en zayıf tarafı tam da StackVo'nun en güçlü tarafı
olan ürün: DDEV'in resmî GUI'si (`ddev-ui`) terk edilmiş durumda ve üçüncü
parti alternatifler hobi projesi seviyesinde.

---

## 1. Sahada ne değişti

| Ürün | 2026-08-01 itibarıyla durum | Sonucu |
| --- | --- | --- |
| **DDEV** | Apache-2.0, DDEV Foundation yönetişimi, iki ücretli bakımcı, ~108 sponsor, 223 sürüm, v1.25.x | Docker tabanlı yerel geliştirmenin ölçütü. Önceki ölçümde hiç yoktu. |
| **Lerd** | Ciddi şekilde olgunlaştı. Rootless Podman, web paneli + TUI + tray + CLI + MCP (~110 aksiyon), git-worktree ortamları | Geçen sefer bir açılış sayfasıydı; şimdi en özellik yoğun ücretsiz rakip. |
| **ServBay** | "AI-native" konumlandırmasına geçti: 39 araçlı MCP, AI Gateway, güvenilir HTTPS alan adıyla Ollama. ServBay 2.0 (Rust çekirdek, Linux) ön gösterimi yapıldı | 2026'nın AI katmanı çıtasını koyuyor. |
| **Laragon** | v7'den (2025) itibaren ticari. Ücretsiz katman uyarı popup'ı gösteriyor; v6 EOL. Fork'lar çıktı | Canlı bir yer değiştirme penceresi — kullanıcıları aktif olarak alternatif arıyor. |
| **XAMPP** | 2023 sonundan beri PHP 8.2.12'de donmuş. Bitnami'nin ücretsiz kataloğu Ağustos 2025'te kapandı, eklenti ekosistemi fiilen yok | Kategorideki en büyük sahipsiz kalmış kullanıcı kitlesi. |
| **Herd** | Pro $99/yıl, Teams $299/yıl. Katalogda Valkey ve RustFS, PHP 8.5 | Hâlâ ergonomi ölçütü. |
| **EnvKit** | Ücretsiz, hesap yok, public beta, Windows + macOS | Herd'e fiyattan saldırıyor; dal (branch) başına gözlemlenebilirlik veriyor. |
| **FlyEnv** | BSD-3, 13+ dil, tek seferlik ~$10 (ya da merge edilmiş bir PR, ya da bir sosyal medya paylaşımı) | Genişlik ve lisans sürtünmesinden saldırıyor. |
| **ForgeKit** | Yalnız Windows, ücretsiz, Tauri + Go — mimari olarak bu uygulamanın birebir aynı şekli, native-binary motorla | Doğrudan UX karşılaştırması; AI katmanı hiç yok. |
| **Laradock** | ~130 servis, kurulum sihirbazlı `./laradock` CLI, üretime `./laradock ship` | Geliştirmeden üretime imaj yolu olan tek diğer ürün. |

Artık *her* native-binary rakibin yaptığı ve her karşılaştırmada beklenmesi
gereken iki iddia var: açılış gecikmesi ve RAM. FlyEnv "Docker'dan %80 az RAM,
<100 ms açılış" yayınlıyor; Laragon <6 MB binary ve ~10 MB RAM yayınlıyor;
ForgeKit ~200 MB çekirdek yayınlıyor. O kavga kazanılamaz kalıyor ve girilmiyor
— ama **I-1**'e bakın, çünkü bunun *dosya G/Ç* yarısı bir pazarlama iddiası
değil, StackVo'nun şu anda düz Docker ile paylaştığı gerçek bir kusur.

---

## 2. Boşluk matrisi — StackVo'nun geride olduğu satırlar

Burada yalnızca yeni satırlar var. Eski matristeki her satır artık StackVo için
`✅` okunuyor ve tekrarlanmıyor.

`✅` var · `⚠️` kısmi · `❌` yok · `–` satıcı iddia etmiyor

| Yetenek | Herd | Lerd | EnvKit | FlyEnv | ServBay | ForgeKit | Laragon | Laradock | DDEV | XAMPP | **StackVo** |
| --- | :-: | :-: | :-: | :-: | :-: | :-: | :-: | :-: | :-: | :-: | :-: |
| Yardımcı CLI | ✅ | ✅ | – | – | ✅ | ⚠️ shim | ⚠️ | ✅ | ✅ | ⚠️ bat | **❌** |
| Repoya işlenen ortam tanımı | ✅ `herd.yml` | ✅ `.lerd.yaml` | – | – | ⚠️ | – | – | ⚠️ `.env` | ✅ `config.yaml` | – | **⚠️** |
| Eklenti (add-on) ekosistemi | – | ✅ preset | – | ✅ modül | – | ⚠️ katalog | ⚠️ conf dosyası | – | ✅ kayıt defteri | ⚠️ Bitnami | **❌** |
| Kullanıcının kendi compose servisi | – | ✅ | – | ⚠️ podman | ⚠️ proxy | – | ⚠️ Procfile | ✅ | ✅ | – | **❌** |
| Yaşam döngüsü hook'ları | – | – | – | – | – | – | – | – | ✅ | – | **❌** |
| Gerçek yerel DNS sunucusu | ✅ | ✅ | – | ✅ | ✅ | – | ⚠️ | – | ✅ `.ddev.site` | – | **❌ hosts** |
| Proje başına çoklu/wildcard alan adı | ✅ | ✅ | – | ✅ alias | ✅ Pro | – | – | – | ✅ `*.x` | – | **❌** |
| LAN paylaşımı | – | ✅ | – | – | ✅ | ✅ sslip | ✅ | – | ⚠️ nip.io | ✅ | **❌** |
| Rastgele bir hedefe reverse proxy | ✅ | ✅ | ✅ | ✅ | ✅ | – | – | – | ⚠️ | – | **❌** |
| Sorgu logu + N+1 tespiti | ✅ Pro | ✅ | ✅ | – | – | – | – | – | – | – | **❌** |
| Job / view / giden HTTP yakalama | ✅ Pro | ✅ | ✅ | – | – | – | – | – | – | – | **❌** |
| Flame graph | ✅ SPX | ✅ SPX | – | – | – | – | – | ⚠️ | ✅ xhgui | – | **❌** |
| Zamanlanmış otomatik veritabanı yedeği | – | – | – | – | ✅ Pro | – | ✅ saatlik | – | – | – | **❌** |
| Adlandırılmış veritabanı snapshot'ı | – | ✅ | – | – | – | – | – | – | ✅ | – | **❌** |
| Masaüstü DB istemcisini bağlı açma | ✅ | – | ✅ | – | – | – | ✅ Heidi | – | ✅ 5 istemci | – | **❌** |
| Hosting sağlayıcıdan pull/push | ✅ Forge | – | – | – | – | – | – | – | ✅ 6+ | – | **❌** |
| Registry push / deploy reçeteleri | ✅ Forge | – | – | – | – | – | – | ✅ 12 hedef | – | – | **❌** |
| Bind-mount performans katmanı | yok | yok | yok | yok | yok | yok | yok | ❌ | ✅ Mutagen | yok | **❌** |
| Boştaki projeyi askıya alma | – | ✅ | – | – | – | – | – | – | – | – | **❌** |
| Nesne depolama (MinIO/RustFS) | ✅ | ✅ | ⚠️ | ✅ | ✅ Pro | – | – | ✅ | ✅ | – | **❌** |
| Arama (Meilisearch/Typesense/Solr) | ✅ | ✅ | ⚠️ | ✅ | ✅ Pro | – | – | ✅ 6 | ✅ | – | **❌** |
| Vektör DB / yerel LLM servisi | – | – | – | ✅ Qdrant | ✅ Ollama | – | – | ✅ 11 | ⚠️ add-on | – | **❌** |
| Aynı servisten birden çok örnek | ✅ Pro | ⚠️ | – | – | ✅ Pro | ✅ | – | – | – | – | **❌** |
| Mail *gönderme* / relay | – | – | – | – | ✅ Pro | – | ✅ Gmail | – | – | ✅ Mercury | **❌** |
| Dal / worktree başına ortam | – | ✅ | ⚠️ kapsam | – | – | – | – | – | – | – | **❌** |
| Agent config / kural yükleyici | ✅ Boost | ✅ 8 istemci | ✅ skill | ✅ | ✅ rules | – | – | – | ⚠️ add-on | – | **❌** |
| Proje grupları / favoriler | ✅ | ✅ | – | – | ✅ Pro | ⚠️ renk | – | – | – | – | **❌** |
| Komut paleti / global kısayol | ✅ OS geneli | ✅ Cmd+K | – | – | – | – | – | – | ⚠️ tui | – | **❌** |
| XAMPP / Laragon / MAMP'ten göç | ✅ rehber | ✅ Sail | ✅ toplu | – | – | ✅ 6 kaynak | ✅ | – | – | – | **❌** |
| Arayüz dili sayısı | 1 | 14 | 5 +RTL | 30+ | çok | 1 | 20+ | – | 1 | 14 | **2** |

### 2.1 StackVo'nun zaten önde olduğu ve önde kalması gereken satırlar

Önceki ölçümden değişmedi ve yeniden doğrulandı: `sysinfo` ile gerçek host
metrikleri, bayt bayt doğrulanmış generator, gözden geçirilmiş yetkili hosts
yazımı, geliştirme imajından türeyen üretim imajı, container **ve** host PTY, ve
yalnızca Laradock'un eşleştiği ağır servis kataloğu (Kafka, Elasticsearch,
Cassandra, Grafana, RabbitMQ) — Laradock'un ise hiç GUI'si yok.

Sahanın görünür kıldığı iki tanesi daha artık isimlendirilmeyi hak ediyor:

- **28 iskelet (scaffold) şablonu, her kurucusu gerçek bir container'da
  ölçülmüş** (`scaffold.rs:84`). Herd `laravel new`'e dayanıyor; DDEV hızlı
  başlangıç *dokümanı* veriyor; Laragon'un Quick app'inde dört giriş var. Bu
  kadarını başka kimse ölçmedi.
- **Tek bir ortak config şekliyle altı runtime** (`project.schema.json`). FlyEnv
  13, ServBay 8 iddia ediyor — ama ikisi de host binary'si yönetiyor, yani her
  dil sonsuza kadar taşıdıkları bir paketleme yükü. StackVo'nunki bir şablon.

---

## 3. Boşluklar, konu başlığına göre

Her madde rakiplerin ne verdiğini, bugün burada ne olduğunu bir dosya
referansıyla ve işin aslında ne olduğunu söylüyor.

### A — Arayüzler: içeri girmenin tek bir yolu var

**A-1. CLI yok.** *EnvKit ve FlyEnv dışında her rakipte var*: `herd`, `lerd`
(artı bir TUI), `ddev` (~50 komut artı `ddev tui`), `./laradock`, `servbayctl`,
`fkit`, `laragon reload`, XAMPP'ın bat dosyaları.

Burada doğrulandı: `src-tauri/src/bin/` içinde tam olarak bir binary var,
`stackvo-mcp.rs`. Bu uygulamayı sürmenin tek iki yolu GUI ve bir MCP sunucusu.
Bu; scripting'i, CI'ı, uzak/SSH oturumlarını ve her README'deki "şunu
terminalinde çalıştır" talimatını kapatıyor.

Maliyeti göründüğünden düşük ve nedeni zaten yazılı: Sprint 16 operasyonları
Tauri'nin event sisteminden ayırdı, `run_operation`'a bir event **sink**'i
vererek — böylece MCP sunucusu onları başsız sürebilsin diye. Bir CLI, aynı
sink'in farklı bir yazıcısı. Komutlar zaten var, zaten doğruluyor, zaten rapor
veriyor. Eksik olan bir argüman ayrıştırıcısı ve bir ilerleme yazıcısı.

**A-2. Komut paleti yok, global kısayol yok.** Herd, *o an gezinilen* proje için
Tinker veya bir terminal açan işletim sistemi seviyesinde kısayol kaydediyor;
Lerd'de Cmd+K var. Burada doğrulandı: frontend'deki tek `keydown` dinleyicisi
`SideSheet.vue:54` ve o da Escape'i işliyor.

**A-3. Host kabuğu entegrasyonu yok.** `herd php`, `fkit php`, `lerd php` ve
`ddev composer` projenin kendi araç zincirini kullanıcının terminalinden,
projenin dizininde çalıştırıyor. StackVo'da container'a bir PTY (`pty.rs`) ve
sabit bir hızlı komut kataloğu (`quickcmd.rs`) var, ikisi de uygulama
penceresinin içinde. Projenin PHP'sini host prompt'una koyan hiçbir şey yok.
Container tabanlı bir araç için bu, native olanlara göre *daha kolay* — bir
`docker exec` sarmalayıcısı — ve A-1'deki CLI'ı değerli kılan şey de bu.

### B — Takım tekrarlanabilirliği: dosya var, akış yok

**B-1. Repoya işlenip ortamı kuran bir tanım yok.** Herd'in `herd.yml`'ı,
Lerd'in `.lerd.yaml`'ı ve DDEV'in `.ddev/config.yaml`'ı ortamın *tamamını*
tarif ediyor — PHP sürümü, alan adları, hangi servis hangi sürümde — ve repoya
işleniyor. Takım arkadaşı klonluyor ve tek komut çalıştırıyor; her şey kuruluyor.

StackVo'da iki yarı da var ve birleşmemişler. `stackvo.json` projeyi taşıyor ve
repoya işleniyor (`contracts/project.schema.json`). Bir preset stack'i taşıyor
ve yapısı gereği sır taşıyamıyor (`preset.rs:19`) — ama o, kullanıcının dışarı
aktarıp elden verdiği bir dosya, klonun içinde olan bir şey değil. Boşluk küçük,
karşılığı büyük: bir proje ihtiyaç duyduğu servisleri beyan edebilsin ve adoption
o beyanı mevcut planla-sonra-uygula yolundan geçirsin. Model Herd'in `herd init`
sihirbazı — projenin `.env`'ini okuyor, servisleri tahmin ediyor ve dosyayı
yazıyor.

**B-2. Makineye özel geçersiz kılma yok.** DDEV'in `config.local.yaml`'ı
gitignore'lu ve repoya işlenenin üzerine merge oluyor. StackVo'nun preset'i
portları ve yolları bilinçli olarak dışarıda bırakıyor çünkü onlar tek bir
makinenin özellikleri — doğru, ama bu onları *koyacak* bir yer bırakmıyor.

**B-3. Yaşam döngüsü hook'u yok.** DDEV neredeyse her komutun (start, stop,
import-db, composer, share, pull, snapshot) etrafında pre/post hook çalıştırıyor.
StackVo'da hiç hook yüzeyi yok: `generator.rs` ve `config.rs` içinde `grep` hiç
bulmuyor. "Proje ayağa kalktıktan sonra migration çalıştır" ifade edilemiyor.

**B-4. Kullanıcı tanımlı komut yok.** DDEV, `.ddev/commands/web/` içindeki bir
shell script'ini yardım metni ve bayraklarıyla birinci sınıf bir `ddev` alt
komuduna dönüştürüyor. StackVo'nun kataloğu derlenmiş ve sabit, ve `quickcmd.rs`
nedenini açıklıyor: frontend bir *id* gönderiyor, argv Rust tarafında derlenmiş
kelimelerden kuruluyor, yani webview asla çalıştırılacak bir programı
adlandıramıyor. Bu gerekçe sağlam ve atılmamalı — ama o gerekçe *webview*'in
seçmesine karşı, *workspace*'in beyan etmesine karşı değil. Bir kez gözden
geçirilip workspace'te saklanan bir komut beyanı, IPC üzerinden gelen bir
string'den farklı bir güven sınırı.

### C — Genişletilebilirlik: hiç uzatma noktası yok

**C-1. Eklenti ekosistemi yok.** DDEV'in bir kayıt defteri (`addons.ddev.com`),
bir kur/kaldır yaşam döngüsü (`ddev add-on get`), 36 resmî ve 100+ topluluk
eklentisi var — dikkat çekici şekilde Claude Code, Copilot ve Cursor'ı proje
container'ının *içine* koyan eklentiler dahil. Lerd'de servis preset'leri, artı
YAML ile kullanıcı tanımlı servisler, artı takılabilir framework tanımları var.
FlyEnv kullanıcının rastgele bir modül tanımlamasına izin veriyor (herhangi bir
binary, YAML config, log sekmesi, kenar çubuğu kategorisi). Laragon'un uygulama
şablonları ve paket kataloğu herkesin Not Defteri'nde düzenleyebileceği düz
metin dosyaları (`sites.conf`, `packages.conf`).

StackVo'nun 21 servisi ve 28 iskelet şablonunun hepsi binary'ye derlenmiş. Bir
tane eklemek bir pull request ve bir sürüm demek. Sprint 23'ün şablonları
`include_dir!` ile gömme kararı kendi kendine yeterlilik için doğruydu ve aynı
zamanda okumaları workspace-öncelikli, derlenmiş kopyayı yedek yaptı
(`skeleton.rs`) — yani kullanıcının kendi servis şablonunun mekanizması *zaten
yarı yarıya var*. Yüzeye çıkarılmamış, dokümante edilmemiş ve yanına koyacak bir
katalog girişi yok.

**C-2. Kullanıcının kendi compose servisi yok.** `runner.rs:243` üç üretilmiş
dosyadan oluşan sabit bir liste döndürüyor ve dört overlay her çağrıda yeniden
türetiliyor. Kullanıcının yazdığı bir dosyayı katmanlayacak yer yok. DDEV
`.ddev/docker-compose.*.yaml`'ı merge ediyor; Lerd bir `Containerfile.lerd`
alıyor; Laradock zaten bir compose dizininden ibaret. Container tabanlı bir araç
için kullanıcının kendi compose dosyasını reddetmek, container tabanlı olmayı
alternatifinden *daha kötü* yapan tek şey.

### D — Servis kataloğu: modern yarısı eksik

Katalog (21 servis, `skeleton/core/templates/services/`) 2018'in ağır stack'inde
güçlü, 2024–2026'nınkinde boş. Eksikler ve kimlerde olduğu:

| Eksik | Kimde var |
| --- | --- |
| MinIO / RustFS (S3 uyumlu nesne depolama) | Herd, Lerd, FlyEnv, ServBay, Laradock, DDEV |
| Meilisearch | Herd, Lerd, FlyEnv, ServBay, Laradock, DDEV |
| Typesense | Herd, FlyEnv, ServBay, Laradock |
| Solr | Laradock, DDEV (üç varyant) |
| Valkey | Herd, Laradock |
| Qdrant / Weaviate / pgvector | FlyEnv, Laradock, ServBay |
| Ollama / LocalAI / vLLM | FlyEnv, ServBay, Laradock |
| ClickHouse | FlyEnv, Laradock |
| Prometheus | Laradock (StackVo'da Grafana var, grafikleyecek şey yok) |
| Keycloak, Selenium, Varnish, MinIO konsolu | Laradock, DDEV |
| MSSQL, Percona, Neo4j, CouchDB | Laradock |

Bunların her biri bir şablon ve bir avuç `.env` anahtarı — bu dokümandaki en ucuz
iş ve container tabanlı olmanın saf avantaj olduğu satır. Native-binary bir
rakip bunların her biri için platform başına bir derleme paketleyip bakımını
yapmak zorunda; StackVo'nun ihtiyacı bir `.tpl`.

İkisi ayrıca anılmayı hak ediyor:

- **Nesne depolama.** Herd, Lerd ve ServBay'in üçünde de var ve hem Herd hem
  Lerd son bir yılda MinIO'dan RustFS'e geçti (MinIO'nun lisanslaması yüzünden).
  Üretimde S3 kullanan herhangi bir projenin şu anda burada yerel karşılığı yok.
- **Yerel AI servisleri.** Önceki analiz "AI Gateway / LLM proxy"yi kapsam dışı
  olarak kapattı ve o karar geçerli — bir LLM *sağlayıcı proxy'si* yerel ortam
  yöneticisinin işi değil. Ama Ollama, Qdrant ve pgvector birer **servis**,
  Redis ile aynı türden nesneler, ve Laradock'un artık on bir container'lık bir
  "AI & ML" kategorisi var. Bu, kapatılan sorudan farklı bir soru ve kendi
  cevabını hak ediyor.

**D-2. Servis başına tek örnek.** Her servisin tam olarak bir sürüm anahtarı var
(`SERVICE_MYSQL_VERSION` vb., `config.rs:116`). Herd Pro servis tipi başına
birden çok adlandırılmış örnek çalıştırıyor ve **birini verisiyle birlikte
klonlayabiliyor**; ServBay Pro eşzamanlı birden çok örnek çalıştırıyor. MySQL
8.0'dan 8.4'e yükseltmeyi denemek şu anda ifade edilemiyor.

### E — Ağ: hosts dosyasına bağlı

**E-1. Yerel DNS sunucusu yok.** Herd, Lerd, ServBay ve FlyEnv dnsmasq
çalıştırıyor; DDEV gerçek public wildcard DNS (`*.ddev.site`) kullanıp hosts
girdisini yedek tutuyor. StackVo `/etc/hosts` yazıyor — evet, gözden geçirilmiş
bir diff ve tek bir yetkili yazımla (`hosts.rs`), ki bu Laragon'un sessiz
otomatik düzenlemesinden gerçekten daha iyi. Ama bu, **her yeni projenin bir
yetkili yazım gerektirdiği** anlamına geliyor ve E-2'yi imkânsız kılıyor.

**E-2. Proje başına tek alan adı.** `project.schema.json`'da tek bir `domain`
string'i var ve şema `additionalProperties: false`. DDEV `*.wildcard`
biçimleri dahil `additional_hostnames` ve ayrıca `additional_fqdns`
destekliyor; Herd site başına birden çok alan adına izin veriyor; Lerd'de
`domain add/remove/list` ve `label.main.test` şeklinde iç içe site grupları var.
Multi-tenant uygulamalar — tenant'ı subdomain'den çözenler — burada hiç
geliştirilemiyor.

**E-3. LAN paylaşımı yok.** ForgeKit site başına bir `sslip.io` URL'si
üretiyor; ServBay `0.0.0.0`'a bağlanıyor ve CA'sını telefonlara/tabletlere
dağıtmayı dokümante ediyor; Lerd'de `lan:share` var; Laragon'un Auto SSL'i LAN
IP'lerini kapsıyor; DDEV `nip.io`'yu dokümante ediyor. StackVo'da public bir
cloudflared tüneli (`tunnel.rs`) var ve localhost ile açık internet arasında
hiçbir şey yok. Telefonda test etmek, projeyi herkese açık hâle getirmek demek.

**E-4. StackVo'nun çalıştırmadığı bir şeye reverse proxy yok.** `herd proxy`,
ServBay'in reverse-proxy site tipi ve Lerd'in host-proxy modu, güvenilir bir
yerel alan adını rastgele bir hedefin önüne koyuyor — host'ta çalışan bir dev
server, başka bir container, uzak bir makine. ServBay'in dokümantasyonu daha da
ileri gidip *Docker container'larını alan adı ve SSL ile önden karşılamayı*
açıkça bir özellik olarak pazarlıyor. StackVo yalnızca kendi ürettiğini
yönlendiriyor.

### F — Gözlemlenebilirlik: en büyük ürün boşluğu

StackVo'da bir dump yakalayıcı (`dumps.rs`, Symfony'nin kendi render'ını akıtan)
ve Xdebug tabanlı bir profiler (`profile.rs`) var. Herd Pro, Lerd ve EnvKit
bunun epey ötesine geçti ve artık üçü de aynı şeyi satıyor:

**F-1. Sorgu logu ve N+1 tespiti.** Herd Pro, EnvKit ve Lerd'in üçü de Eloquent
sorgu logunu süreleriyle yakalıyor, EnvKit ve Lerd ise N+1 örüntüsünde
*bildirim* çıkarıyor. Bu, üç ürünün pazarlamasında da en çok anılan özellik.

**F-2. Tek akış değil, tek zaman çizelgesi.** Herd'in dumps penceresi ayrıca
dispatch edilen job'ları, render edilen Blade view'larını verileriyle, giden
HTTP isteklerini, yakalanan mailleri ve istek kapsamlı logları kategori bazlı
açma/kapama ile taşıyor. Lerd'in devtools toplayıcısı aynısını yapıyor.
StackVo'da dump'lar bir ekranda (`dumps_open`), mail başka birinde
(`Mail.vue`), loglar bir üçüncüsünde (`Logs.vue`) — aralarında korelasyon ve
istek kavramı yok.

**F-3. Flame graph yok.** Önceki analizde dürüstçe söylenmişti ve hâlâ geçerli:
`profile.rs` cachegrind'i en pahalı fonksiyon tablosuna indirgiyor, çünkü çağrı
ağacı için çağıran kenarlarının yeniden kurulması gerekiyor. Herd ve Lerd SPX
flame graph veriyor; DDEV çağrı grafikli XHGui veriyor.

**F-4. Xdebug bir anahtar, bir dedektör değil.** Herd PhpStorm'un
`.idea/workspace.xml`'ini ayrıştırıp konulmuş breakpoint'leri buluyor ve
*yalnızca o istekleri* Xdebug açık bir sürece yönlendiriyor, böylece hata
ayıklamadığınızda hata ayıklamanın maliyeti sıfır oluyor. StackVo'nun
`xdebug_set`'i proje başına bir aç/kapa ve yeniden build gerektiriyor.

**F-5. Kendine ait bir REPL yüzeyi yok.** Lerd, Monaco'yu özel bir Rust PHP dil
sunucusuyla gömüyor ve her ifadenin çalıştırdığı SQL'i yakalıyor. StackVo
`artisan tinker`'ı PTY üzerinden çalıştırıyor — ki bu dürüst %90'dır ve öyle
denmeli — ama bir terminal, bir tezgâh değil.

### G — Veritabanları: yarat ve yok et, arası boş

`db.rs` içinde `db_dump` ve `db_restore` var, başka bir şey yok. Doğrulandı:
zamanlayıcı yok, snapshot kaydı yok, bağlantı dizesi yardımcısı yok.

- **G-1. Zamanlanmış yedek yok.** ServBay Pro günlük/haftalık/aylık, saklama
  penceresi ve harici disk hedefiyle çalışıyor. Laragon MySQL veri dizinini
  **saatlik** yedekliyor ve beş tane tutuyor. İkisi de bunu veri güvenliği
  olarak satıyor ve ikisi de haklı: yerel bir veritabanı, kimsenin commit
  etmediği emeği tutar.
- **G-2. Adlandırılmış snapshot yok.** `ddev snapshot` ve `lerd db:snapshot` bir
  zaman noktasını adlandırıp adıyla geri yüklüyor; DDEV her projeyi aynı anda
  snapshot alabiliyor. Kullanıcının seçtiği bir yola alınan dump ham madde,
  özelliğin kendisi değil.
- **G-3. Masaüstü istemci açma yok.** `ddev tableplus | sequelace | dbeaver |
  querious | heidisql` istemciyi zaten bağlı açıyor; EnvKit'te "Şununla aç"
  düğmesi var; Herd site menüsünden TablePlus veya AdminerEvo açıyor. StackVo
  Adminer ve phpMyAdmin'i container olarak veriyor ve kimlik bilgilerini
  `Services.vue`'da gösteriyor — yani kullanıcı bunları tekrar yazıyor.
- **G-4. Servisler arası taşıma veya sürüm göçü yok.** `lerd db:move` bir
  veritabanını servis örnekleri arasında taşıyıp `.env`'i yeniden işaret ediyor;
  `lerd service migrate` veriyi bir sürüm yükseltmesi boyunca taşıyor.

### H — Üretim köprüsünün yarısı yapılmış

Sprint 11, soyu geliştirme imajı olan bir üretim imajı yaptı ve çalıştırıp
sorarak temiz olduğunu (`.env` dosyası yok, Xdebug yok) kanıtladı. Zor yarısı bu
ve sahada Laradock dışında kimsede yok.

Kolay yarısı eksik. `release.rs` yerel olarak build edip kaydediyor: registry
push yok, tag-and-publish yok, deploy reçetesi yok. Laradock'un `ship`'i on iki
hedefi dokümante ediyor (Cloud Run, ECS, Fly.io, Render, Railway, Kubernetes,
Kamal…). Herd tam ters yöne gidip Forge ile entegre oluyor — tek tıkla deploy,
sunucuya SSH ve **üretimdeki `.env`'i yerele çekme**.

**H-1. Hosting sağlayıcıdan pull/push yok.** Bu DDEV'in en yapışkan özelliği ve
ajansların onu seçme nedeni: `ddev pull` üretim veritabanını ve kullanıcı
dosyalarını Acquia, Pantheon, Platform.sh, Upsun, Lagoon, rsync ya da özel bir
reçeteden çekiyor; `ddev push` geri gönderiyor. Burada makinenin dışına uzanan
hiçbir şey yok.

### I — Performans: Docker eleştirilerinin doğru olanı

**I-1. Bind-mount performans katmanı yok.** DDEV **Mutagen**'i paketliyor ve
macOS ile Windows'ta varsayılan olarak açıyor, kullanıcıya açık kontrollerle
(`ddev mutagen sync | status | monitor | reset`, bir teşhis komutu ve proje
başına `performance_mode`). Nedeni şu: macOS ve Windows'ta bind-mount edilmiş
kaynak kod, insanların Docker tabanlı bir iş akışını bırakmasının en yaygın tek
nedeni olacak kadar yavaş — ve bu, her native-binary rakibin saldırdığı zeminin
ta kendisi. StackVo doğrudan bind-mount yapıyor.

Bu dokümandaki en sonuç doğurucu madde. Diğer her boşluk bir özelliğe mal
oluyor; bu, *argümana* mal oluyor. Burada Herd'dekinin 4 katı süren bir Laravel
test suite'ine "tekrarlanabilirlik" cevap değildir.

**I-2. Boşta askıya alma yok.** Lerd kullanılmayan siteleri yapılandırılabilir
bir zaman aşımıyla, site başına sabitleme seçeneğiyle askıya alıyor ve bu arada
bir açılış sayfası sunuyor — RAM karşılaştırmasını "5 proje çalışıyor"dan "1
proje çalışıyor"a çeviriyor. StackVo'nun projeleri durdurulana kadar çalışıyor.

### J — Runtime'lar: iyi, iki delikle

Altı runtime, PHP 5.6–8.5, Node 16–23 ve tespit anında okunan
`.nvmrc`/`engines.node` (`detect.rs:450`) — bu satır rekabetçi ve önceki
ölçümde hakkı yenmiş. İki şey eksik:

- **J-1. Bun yok, Deno yok.** ServBay, FlyEnv ve Lerd'in üçünde de var; Lerd bir
  sitenin `bun | node | auto` sabitlemesine izin veriyor.
- **J-2. Corepack / paket yöneticisi sabitleme yok.** DDEV'in `corepack_enable`'ı
  repo'nun beyan ettiği pnpm veya yarn sürümünü sabitliyor. StackVo'nun node
  şablonu npm çalıştırıyor.

### K — AI katmanı: titizlikte önde, erişimde geride

StackVo'nun MCP sunucusu `--allow-writes` arkasında yazma yüzeyiyle 17 araç
veriyor (`mcp.rs`) ve üç kontrat testi her aracı `contracts/ipc.json`'a karşı
çapraz kontrol ediyor — var olmayan bir komutu adlandıran bir araç build'i
kırıyor. **Hiçbir rakip MCP yüzeyini kontrol edilen bir kontrattan türetmiyor**
ve bu gerçek bir farklılaştırıcı olarak duruyor.

Ama: ServBay 39 araç veriyor, Lerd ise test çalıştırma, sorgu analizi,
profilleme ve worktree yönetimi dahil ~110 aksiyonu kapsayan on iki gruplu araç
veriyor. Ve hepsi StackVo'nun yapmadığı son adımı yapıyor:

- **K-1. Agent yapılandırma yükleyicisi yok.** `lerd mcp:enable-global` sekiz
  istemci için (Claude Code, Cursor, Junie, Codex, Gemini CLI, Copilot,
  Antigravity, Windsurf) bağlam dosyaları yazıyor ve `lerd mcp:inject` takım için
  makineden bağımsız bir config'i repoya işliyor. ServBay, agent'lara MCP'sini
  güvenli kullanmayı anlatan "AI Rules"u kuruyor. EnvKit Claude skill dokümanı
  gönderiyor. Herd, Laravel Boost üzerinden kuruyor. StackVo'nun README'si
  kullanıcıdan bir JSON bloğunu elle yazmasını istiyor. Bu dokümandaki en ucuz
  madde.
- **K-2. Container içinde agent farkındalığı yok.** Lerd agent tespit
  değişkenlerini (`CLAUDECODE`, `CURSOR_AGENT`) PHP'ye geçiriyor ki paketler
  yapılandırılmış çıktı dönebilsin. DDEV'in topluluk eklentileri Claude Code,
  Copilot ve Cursor'ı web container'ının *içine* kuruyor.

### L — Onboarding: göç penceresi açık ve sahipsiz

StackVo bir klasörü sahiplenebiliyor (tespit) ve bir `docker-compose.yml`
okuyabiliyor (`migrate.rs`). Hiçbir rakibin yapılandırmasını okuyamıyor.

Bu arada: **XAMPP 2023'ten beri PHP 8.2'de donmuş ve Eylül 2025'te eklenti
ekosistemini kaybetti**, **Laragon 2025'te uyarı popup'larıyla ticarileşti ve
fork'landı**. Bunlar kategorideki en büyük iki kurulu taban ve iki kitle de
arayışta. Her ciddi rakip onlara açıkça kur yapıyor: EnvKit Laragon'u
veritabanları ve PHP eklentileri dahil toplu içe aktarıyor (ve Laragon'u
`PATH`'ten siliyor), ForgeKit altı göç kaynağı listeliyor, Herd Sail ve MAMP
için göç rehberi yayınlıyor, Lerd'de `lerd import sail` var.

Bir Laragon içe aktarıcısı mekanik bir iş: proje başına vhost dosyaları
`etc/apache2/sites-enabled/auto.*.conf` altında, projeler `www/` içinde ve veri
dizini `data/`. Bir XAMPP içe aktarıcısı daha da kolay — `htdocs/` alt dizinleri
ve bir `my.ini`.

### M — Daha küçük maddeler, her biri ucuz

| # | Boşluk | Kimde var |
| --- | --- | --- |
| M-1 | Proje grupları ve favoriler | Herd, Lerd (workspace), ServBay Pro |
| M-2 | Mail *gönderme* / relay — yakalanan maili gerçek alıcıya iletme | Laragon (Gmail), ServBay Pro (relay, webhook, SpamAssassin), XAMPP (Mercury) |
| M-3 | Paylaşım URL'sinde QR kod, telefonda test için | Laragon |
| M-4 | Her siteyi listeleyen bir açılış sayfası | ForgeKit, Lerd (çevrimdışı PWA açılış sayfası) |
| M-5 | Proje başına ortam değişkenleri | ServBay (`.servbay.config`), DDEV (`web_environment`), FlyEnv |
| M-6 | Proje başına dizin listeleme anahtarı | Herd, ForgeKit |
| M-7 | Arayüz dilleri — StackVo'da 2 (`src/i18n/locales/`) | FlyEnv 30+, Laragon 20+, Lerd 14, XAMPP 14, EnvKit 5 + RTL |
| M-8 | Alternatif yüzeyler: TUI, yalnız tray, PWA paneli | Lerd (üçü de), DDEV (`ddev tui`) |
| M-9 | Sabit katalog ötesinde framework geçiş komutları (`ddev drush`, `ddev wp`) | DDEV, Lerd |
| M-10 | SSH agent'ının container'a iletilmesi | Lerd, DDEV (`ddev auth ssh`) |
| M-11 | Stripe webhook dinleyicisi | Lerd |
| M-12 | `.loc` alan adları için OAuth callback yönlendirme | Herd (`fwd.host`) |

### N — Lerd dışında kimsede olmayan ve StackVo'nun konumlandığı yer

**Worktree başına ortam.** `git worktree add` dala kendi subdomain'ini, kendi
veritabanını (boş-ve-migrate ya da main'den klonlanmış), kendi dev server
portunu, dalın URL'siyle yeniden yazılmış bir `.env`'ini ve lock dosyaları
eşleştiğinde reflink ile tohumlanmış vendor dizinlerini veriyor. Lerd'in amiral
gemisi ve sahada buna yaklaşan başka hiçbir şey yok.

Ayrıca container tabanlı bir araç için Podman tabanlı olandan *daha* doğal — dal
başına veritabanı izolasyonu bir volume adı, dal başına yönlendirme bir Traefik
kuralı. StackVo'nun kimsenin hızlıca kopyalayamayacağı tek bir özellik istemesi
hâlinde aday bu.

---

## 4. Önerilen sıra

Matristeki konuma göre değil, doğrulanmış duruma karşı etki ÷ efor ile
sıralandı.

### P0 — argümanı değiştirenler

1. **Bind-mount performansı (I-1).** Günlük döngü yavaşsa bu listedeki başka
   hiçbir şeyin önemi yok. Önce ölçün, macOS'ta gerçek bir Laravel test
   suite'ine karşı: düz bind vs `:cached`/`delegated` vs bir senkron katman.
   Ölçümün kendisi ilk teslimat *olmalı* — bunun bir mount bayrağı değişikliği
   mi yoksa Mutagen sınıfı bir alt sistem mi olduğuna o karar veriyor.
2. **Bir CLI (A-1).** On rakibin sekizinde var; altyapı zaten yerinde çünkü
   Sprint 16'nın event sink'i tam olarak bunun için yapıldı. Ayrıca CI'ı,
   scripting'i ve her README talimatını açıyor.
3. **Servis kataloğu: nesne depolama, arama, vektör (D-1).** Bu dokümandaki en
   ucuz yüksek etkili iş — şablonlar ve `.env` anahtarları, yeni mekanizma yok.
   Altı rakibin ayrı ayrı verdiği MinIO/RustFS ve Meilisearch ile başlayın.
4. **Agent yapılandırma yükleyicisi (K-1).** Bir günlük iş. Elle düzenleme
   istemek yerine MCP bloğunu var olan istemcilerin config dosyalarına yazar.

### P1 — tekrarlanabilirlik hikâyesini kapatmak

5. **Repoya işlenen ortam tanımı (B-1)** — `stackvo.json`'ı preset'e bağlayın ki
   bir klon kendi stack'ini mevcut planla-sonra-uygula yolundan kursun.
6. **Kullanıcının kendi compose servisi ve şablonu (C-1, C-2).** `skeleton.rs`
   içindeki workspace-öncelikli okuma mekanizmanın yarısını zaten sağlıyor.
7. **Proje başına çoklu alan adı ve wildcard (E-2)** — şemanın açılmasını ve
   hosts yazıcısının SAN listelerini öğrenmesini gerektiriyor. Multi-tenant
   uygulamalar şu anda hiç geliştirilemiyor.
8. **Adlandırılmış DB snapshot'ları ve zamanlanmış yedekler (G-1, G-2).**
   `db_dump` var; eksik olan bir kayıt defteri ve bir zamanlayıcı.
9. **XAMPP ve Laragon içe aktarıcıları (L).** Pencere şu anda açık ve açık
   kalmayacak.

### P2 — derinlik

10. **Sorgu logu ve N+1 tespiti (F-1)**, ardından tek istek zaman çizelgesi
    (F-2). Bu üç rakibin amiral gemisi ve en büyük *ürün* boşluğu, ama
    container içinde bir toplayıcı gerektiriyor — P0 olmamasının nedeni bu.
11. **Yaşam döngüsü hook'ları (B-3)** ve workspace tarafından beyan edilen
    komutlar (B-4).
12. **LAN paylaşımı (E-3)** ve reverse-proxy hedefleri (E-4).
13. **Registry push ve deploy reçeteleri (H)** — pahalı yarısı zaten bitmiş bir
    özelliğin ucuz yarısı.
14. **Masaüstü DB istemcisi açma (G-3)**, proje grupları (M-1), komut paleti
    (A-2).

### P3 — taban sağlamlaştığında, farklılaştırıcı

15. **Worktree başına ortam (N).** Pahalı, ve buradaki StackVo'yu sahayla
    eşitlemek yerine sahanın önüne geçirecek tek madde.

---

## 5. Tuzaklar

Önceki ölçümden yeniden onaylananlar, artı iki yeni.

- **Native-binary hız savaşı.** Hâlâ kazanılamaz, hâlâ girmeye değmez. Ama
  I-1'in çizdiği ayrımı not edin: *soğuk açılış* kaybedilen bir tartışma, *dosya
  G/Ç* gerçek bir kusur. Birincisi ikincisini görmezden gelmenin bahanesi
  olmasın.
- **Bir LLM sağlayıcı proxy'si** (ServBay'in AI Gateway'i, FlyEnv'in asistanı).
  Hâlâ kapsam dışı. Yerel AI *servisleri* (Ollama, Qdrant, pgvector) farklı bir
  soru — D-1'e bakın.
- **FlyEnv'in 50+ aracı** (base64, QR kod, regex test ediciler). Hâlâ odaksız.
- **Portable mod.** Docker bağımlılığıyla hâlâ anlamsız.
- **Yeni: Laradock'un 130 servisinin peşine düşmek.** Genişliğin kendisi için
  genişlik, bir kataloğun bakımsız hâle gelme yolu. D-1'deki on iki tanesi bir
  sayı iyi görünsün diye değil, altı rakibin ayrı ayrı verdiği için seçildi.
- **Yeni: ücretli katmanlar.** Herd $99/yıl, ServBay $59/yıl alıyor, Laragon
  ticarileşti ve bu yüzden fork'landı. EnvKit, ForgeKit ve DDEV tam oradan
  saldırıyor. MIT o çizginin doğru tarafı olarak kalıyor.

---

## 6. Bu dokümanın istediği kararlar

Aşağıdaki üç şey sadece "yapılmamış" değil — kayıtlı bir kararla çelişiyor ya da
onu genişletiyor, ve sessizce planlanmak yerine açıkça cevaplanmalı.

1. **Yerel AI servisleri.** "AI Gateway / LLM proxy" kapsam dışı olarak
   kapatıldı ve kapalı kalmalı. Ollama, Qdrant ve pgvector'ün *katalog servisi*
   olması hiç sorulmamış farklı bir soru. (D-1)
2. **Kullanıcı uzatma noktaları.** `quickcmd.rs` haklı olarak webview'in asla
   çalıştırılacak bir programı adlandıramayacağını savunuyor. *Workspace*'in bir
   servis şablonu veya komut beyan edip edemeyeceği — bir kez gözden geçirilip
   diske yazılan — ayrı bir güven sorusu ve cevabı C-1, C-2 ve B-4'ü birlikte
   karara bağlıyor.
3. **İkinci bir arayüz.** Bir CLI eklemek, kontratla senkron tutulacak ikinci
   bir yüzey demek. E ve F suite'leri tam da bu tür bir kaymayı durdurmak için
   var ve MCP sunucusu desenin genişlediğini zaten kanıtladı. Sonradan değil,
   önceden onaylanmaya değer.
