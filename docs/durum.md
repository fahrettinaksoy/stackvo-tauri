# StackVo — durum, kararlar ve kalan işler

**Son ölçüm: 11 Ağustos 2026.** `docs/` altındaki tek doküman budur.

## Bu dosya ne

Beş dokümanın yerini alıyor: iki rekabet analizi, bir kurumsal olgunluk
incelemesi, bir platform matrisi ve on ADR. Onlarda ne olduğu **§6'da**, hangi
kararların verildiği **§5'te**, neyin bitip neyin kaldığı **§2–§4'te**.

Sıkıştırıldı, atılmadı: kararların gerekçesi ve yolda bulunan hatalar burada
duruyor, çünkü bir kararın *neden* öyle verildiği bir sonraki okuyucunun
ihtiyaç duyduğu tek şey. Silinen ayrıntı — rakip rakip özellik tabloları, aynı
tespitin üç kez anlatımı — git geçmişinde.

**Numaralar korundu.** Koddaki yorumlar "ADR 0005", "ADR 0009" diye atıf
yapıyor; §5'teki tablo aynı numaraları taşıyor, yani o atıflar hâlâ bir yere
gidiyor.

## Nasıl ölçüldü

Her durum satırı bugün ağaca karşı kontrol edildi, hatırlanarak değil. "Nasıl
bakıldı" sütunu, bir sonraki okuyucunun aynı kontrolü tekrarlayabilmesi için
var — pahalı yoldan öğrenilmiş bir ders: bir turda kalan-işler tablosunun altı
satırı yanlış çıktı ve biri hiç açık değildi. Bir kontrolün *yapıldığının*
yazılı olması, yapıldığı anlamına gelmiyor.

**§2–§4'ün arkasında bir kapı yok ve olamaz.** "Yapılmadı" kodun ölçülebilir
bir özelliği değil, bir niyetin kaydı. §5 ve §7'nin arkasında **var**: karar
tablosu ve ölçüm tablosu testlerle tutuluyor, yanlış bir sayı build'i kırıyor.

`✅` bitti · `🟡` yarım (ne yarım olduğu yazılı) · `⬜` başlanmadı ·
`⛔` engelli (dışarıdan bir şey gerekiyor) · `🔒` karar bekliyor

---

## 1. Teslim edilenler

Rekabet incelemesinin P0–P1 kuyruğundan altı madde. Her birinin altında yalnızca
**karar** ve **yolda bulunan hata** duruyor; yapılan işin kendisi koddadır.

### D-1 — servis kataloğu (21 → 25)

MinIO (nesne depolama), Meilisearch ve Typesense (arama), Valkey (Redis'in
çatalı). Altı rakibin ayrı ayrı verdiği satırlar.

Bir servisin kataloğa girmesi için dokunulan dokuz yer: şablon dizini,
`template.rs`'in `DYNAMIC_SERVICES`'i (sıra dahil — dosya bu sırayla
birleştiriliyor), `config.rs`'in `EMBEDDED`'ı, `env.schema.json`'ın kategorileri,
`commands.rs`'in `RENDERED`'ı, `connect.rs`, `migrate.rs`, i18n, golden fixture.

**Yolda bulunan:** bir arama motorunun kimlik bilgisi bir **anahtar**, parola
değil. `SERVICE_MEILISEARCH_MASTER_KEY` ve `SERVICE_TYPESENSE_API_KEY`,
`Env::is_secret`'in beş sonekinin hiçbirine uymuyordu — o liste tek yerde durup
dört mekanizmayı besliyor (Services sayfasının maskesi, `redacted()`, log
temizleyici, `secrets::is_movable`), yani indeksin tamamını açan anahtar
maskesiz ekrana ve loglara gidecek, keystore'a da taşınamayacaktı. Sonek
listesine `KEY` eklendi.

**Kararlar:** Valkey'in host portu 6381 (Redis'le yan yana çalışması istenen tek
sebep); MinIO'nun alan adı konsola gider, S3 API'sine değil (SDK zaten bir
endpoint tutuyor); Meilisearch'ün `MEILI_ENV`'i `development`'a sabit (üretim
değeri 16 bayttan kısa anahtarda başlamıyor ve bu, çıkan bir container olarak
öğrenilir).

### K-1 — agent yapılandırma yükleyicisi

Settings → AI assistants: Claude Code, Claude Desktop, Cursor, Windsurf, VS
Code, Gemini CLI. README elle JSON yapıştırmayı istiyordu.

**Üç kural**, çünkü düzenlenen dosya bize ait değil: (1) oku, tek anahtar ekle,
geri yaz — şablondan config üretme, bilinmeyen anahtarlar hayatta kalsın;
(2) ayrıştırılamayan dosya düzenlenmez — VS Code'un `mcp.json`'ı yorum satırlı
JSON, ve yorumları temizlemek kullanıcının kendi notlarını silmektir, o yüzden
durum bildirilip yapıştırılacak blok veriliyor; (3) yazmadan önce yanına
`.stackvo-backup`.

`stackvo-mcp` uygulamayla gelmiyor, o yüzden aranıyor; bulunamazsa kayıt
**reddediliyor** — var olmayan bir yolu yazmak, başlamayan bir sunucu bildiren
istemci demek ve sebebi kimsenin görmediği bir logda durur.

`--allow-writes` listenin üstünde, kapalı, ve ne verdiğini adıyla söylüyor
(`stack_down` dahil). `ipc.js` sarmalayıcısının da varsayılanı yok: sarmalayıcıda
verilmiş bir güvenlik kararı, verilmemiş bir karardır.

Codex (TOML) ve Zed (doğrulanamayan biçim) bilerek dışarıda.

### B-1 — repoya işlenen ortam tanımı

`stackvo.json` → `services`. Klonlayan kişi projeyi açıyor, listeyi görüyor,
eksik olanı bir tıkla açıyor.

İşin çoğu yazılmıştı: bir preset ile repoya işlenmiş bir beyan **aynı cümlenin
iki kişi tarafından söylenmiş hâli**, o yüzden beyan `preset::Preset`'e çevrilip
mevcut planlayıcıya veriliyor. İki asimetri korundu — beyan asla **kapatmaz** (A
projesinin Redis'e ihtiyacı olmaması B'ninkini durdurmaz) ve **sürüm sabitlemez**
(çalışma alanında servis başına tek `VERSION`).

Beyanı ilk kez kimse elle yazmasın diye `.env`'den çıkarım, ve muhafazakâr
olmak zorunda: **Laravel her `.env.example`'da `REDIS_HOST=127.0.0.1`
gönderiyor**, kullanılsın kullanılmasın. Anahtarın varlığına bakan bir kural
klonlanmış her Laravel projesine Redis yazardı. O yüzden iki tür kanıt sayılıyor
— değeri bir servisi adlandıran sürücü anahtarı, ve bu makineden başkasını
gösteren host anahtarı. Çıkarım **anahtar adını** taşıyor, değeri değil.

Arayüzde beyan edilen (commit edilmiş) ile önerilen (tahmin) ayrı duruyor. Bir
tahmini taahhüt gibi göstermek, bir reponun kimsenin seçmediği bir servisi beyan
etmesinin yoludur.

### E-2 — proje başına çoklu ve joker alan adı

`stackvo.json` → `aliases`. Bir isim üç yerde bayt bayt karşılaştırılıyor ve
üçü de listeyi okuyor: Traefik kuralı, sertifika SAN'ı, hosts satırları.

**Joker `/etc/hosts`'a giremez** — hiçbir hosts dosyası joker ifade edemez. Bu
çözücünün bir özelliği; DDEV bunu gerçek public DNS ile, Herd/Lerd dnsmasq ile
çözüyor, StackVo'da ikisi de yok (E-1). O yüzden joker sertifikaya ve
yönlendiriciye giriyor, hosts yazıcısına girmiyor, ve bu her katmanda yazılı.

**Yolda bulunan:** ilk sürüm `HostRegexp(\^[a-zA-Z0-9-]+\.shop\.loc$\)` üretiyordu
— regexp olarak doğru, compose dosyası olarak **hiç ayrıştırılamaz**: kural bir
etikete giriyor, etiket çift tırnaklı YAML skaleri, ve `\.` YAML'da geçerli bir
kaçış değil. Tek bir proje joker beyan ettiği anda diğer bütün projeler dahil
hiçbir şey kalkmıyordu. Düzeltme ters bölüyü ikilemek değil kaldırmak oldu:
`[.]` her motorda aynı karakter sınıfı ve YAML'dan, Docker etiketinden, Go'dan
dokunulmadan geçiyor.

Gerçek Traefik'e sorulan sonuç: `shop.loc` 200, `tenant1.shop.loc` 200,
`a.b.shop.loc` **404** (joker bir etiket derinliğinde — RFC 6125, `san_covers`
ile aynı), `x.shop.loc.attacker.test` **404** (desen sabitli), `shopXloc`
**404** (noktalar karakter sınıfı).

**Yolda kapatılan veri kaybı:** `formToSpec` bütün manifesti üretiyor, yani
formun taşımadığı alan Kaydet'in sildiği alandır. `services` ve `aliases` ikisi
de o yoldan gidecekti — kullanıcı PHP sürümünü değiştirip kaydettiğinde beyanı
kendi reposundan sessizce silinirdi.

### G-1 / G-2 — snapshot ve zamanlanmış yedek

Kayıt defteri **dizinin kendisi**; indeks dosyası yok. Bir indeks, "hangi
snapshot'lar var" sorusuna ikinci bir cevap olurdu ve biri Finder'da bir dosyayı
sildiği ilk anda kayardı.

**Saklama penceresi kimsenin adlandırdığı bir snapshot'ı silmez.** Zamanlanmışlar
`auto-` önekli, `safe_name` bir kişinin o önekle ad yazmasını reddediyor, ve elle
adlandırılmışlar pencereye sayılmıyor da (beş tanesi zamanlayıcının kendi
kopyalarını hiç budayamamasına yol açardı).

**Cron değil**, ve olmaması bir karar: aralık son snapshot'tan ölçülüyor, o
yüzden üç gün kapalı kalmış bir dizüstü üç değil bir snapshot borçlu. Hiç yoksa
hemen zamanı gelmiştir; gelecekte duran bir zaman damgası (saat düzeltmesi)
zamanlayıcıyı durdurmuyor; yalnızca çalışan veritabanları yedekleniyor.
Zamanlayıcı hiçbir şey için hata göstermiyor — Docker kapalı diye diyalog açan
bir yedekleme özelliği, insanların kapattığı bir özelliktir.

### L — XAMPP ve Laragon'dan içe aktarma

Bir özellik listesi değil bir **pencere**: XAMPP 2023'ten beri PHP 8.2'de donmuş
ve Eylül 2025'te eklenti ekosistemini kaybetti; Laragon 2025'te ticarileşti ve
fork'landı.

İçe aktarma bir **dosya işlemi**, ardından mevcut sahiplenme yolu — üretici
`${PROJECTS}/<ad>`'ı bind-mount ediyor, yani bir proje projeler dizininin
altında yaşar ya da yoktur. Komut manifest yazmıyor; `project_adopt` çağrılıyor,
böylece içe aktarılmış proje ikinci sınıf değil.

**Kopyalama varsayılan**, taşıma teklif ediliyor: birinin sitesini hâlâ kurulu
duran bir XAMPP'ın altından çekip almak, karşılaştırma yaptığı kurulumu bozar.
Taşımada önce kopyalanıyor, asıl ancak kopya bittikten sonra siliniyor — ters
sıra dolu bir diski her iki yerde de olmayan bir siteye çevirir.

**Diğer kuruluma tek bayt yazılmıyor.** EnvKit, Laragon'u içe aktarırken onu
`PATH`'ten siliyor; bu, başkasının makinesi hakkında onun adına verilmiş bir
karar.

Laragon'un vhost'u alan adını veriyor ama `ServerAlias` bilerek okunmuyor: aynı
sitenin ikinci adı ve manifestte tek `domain` var — fazladan adlar E-2'nin
`aliases`'ına ait. Sembolik bağ izlenmiyor: `/` gösteren biri, bir sitenin
kopyasını diskin kopyasına çevirir.

---

## 2. Rekabet boşlukları — kalan

Sahadaki on ürüne karşı ölçüldü (Herd, Lerd, EnvKit, FlyEnv, ServBay, ForgeKit,
Laragon, Laradock, DDEV, XAMPP). **Mimari olarak en yakın rakip DDEV** — Docker
tabanlı, proje başına stack, paylaşılan Traefik router, mkcert HTTPS, repoya
işlenen config — ve en zayıf tarafı tam da StackVo'nun en güçlüsü: resmî GUI'si
terk edilmiş durumda.

### A — Arayüzler: içeri girmenin tek yolu var

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| A-1 | Yardımcı CLI | ⬜ | `src-tauri/src/bin/` yalnızca `stackvo-mcp.rs` |
| A-2 | Komut paleti / global kısayol | ⬜ | tek `keydown` dinleyicisi `SideSheet.vue` (Escape) |
| A-3 | Host kabuğu entegrasyonu (`stackvo php …`) | ⬜ | A-1'in arkasında |

On rakibin sekizinde CLI var. Maliyeti göründüğünden düşük: `progress.rs`'in
`ProgressSink`'i ve `Sink::Null` sayesinde MCP yolu hiçbir Tauri tipi
adlandırmıyor, yani ayrıştırma yapılmış — eksik olan bir argüman ayrıştırıcısı
ve bir ilerleme yazıcısı. §5'teki karar isteniyor.

### B — Takım tekrarlanabilirliği

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| B-1 | Repoya işlenen ortam tanımı | ✅ | — |
| B-2 | Makineye özel geçersiz kılma (`config.local`) | ⬜ | preset portları ve yolları bilinçli olarak dışarıda bırakıyor; koyacak yer yok |
| B-3 | Yaşam döngüsü hook'ları | ⬜ | `generator.rs` + `config.rs` içinde `hook` sıfır isabet |
| B-4 | Kullanıcı tanımlı komut | 🔒 | §5'teki karara bağlı |

### C — Genişletilebilirlik: hiç uzatma noktası yok

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| C-1 | Kullanıcının kendi servis şablonu | 🔒 | `skeleton.rs`'in workspace-öncelikli okuması mekanizmanın yarısını veriyor; yüzey yok |
| C-2 | Kullanıcının kendi compose servisi | 🔒 | `custom.yml` / overlay sıfır isabet |

DDEV'in kayıt defteri (`addons.ddev.com`), 36 resmî ve 100+ topluluk eklentisi
var. Container tabanlı bir araç için kullanıcının kendi compose dosyasını
reddetmek, container tabanlı olmayı alternatifinden *daha kötü* yapan tek şey.

### D — Servis kataloğu

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| D-1 | Nesne depolama, arama | ✅ | — |
| D-1 | Solr, ClickHouse | ⬜ | şablon dizini yok |
| D-1 | Ollama, Qdrant, pgvector | 🔒 | §5'te **ertelendi** olarak kayıtlı, kapsam dışı değil |
| D-2 | Aynı servisten birden çok örnek | ⬜ | `env.schema.json`'da `instance` kavramı yok |

### E — Ağ: hosts dosyasına bağlı

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| E-1 | Gerçek yerel DNS sunucusu | ⬜ | `dnsmasq` sıfır isabet |
| E-2 | Proje başına çoklu/joker alan adı | 🟡 | Teslim edildi; **joker `/etc/hosts`'ta çözülmüyor** — E-1'in buradan görünen hâli |
| E-3 | LAN paylaşımı | ⬜ | `sslip`/`nip.io` sıfır isabet |
| E-4 | Rastgele bir hedefe reverse proxy | ⬜ | yalnız kendi ürettiği yönlendiriliyor |

E-1 her yeni projenin bir yetkili yazım gerektirmesi demek; ve joker adların tek
gerçek çözümü o.

### F — Gözlemlenebilirlik: en büyük ürün boşluğu

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| F-1 | Sorgu logu + N+1 tespiti | ⬜ | `query_log` / `n+1` sıfır isabet |
| F-2 | Tek istek zaman çizelgesi | ⬜ | dump/mail/log üç ayrı ekran, korelasyon yok |
| F-3 | Flame graph | ⬜ | `profile.rs` cachegrind'i en pahalı fonksiyon tablosuna indiriyor |
| F-4 | Xdebug'ın anahtar değil dedektör olması | ⬜ | proje başına aç/kapa + rebuild |
| F-5 | Kendine ait REPL yüzeyi | ⬜ | PTY üzerinden `tinker` — dürüst %90, ama tezgâh değil |

Herd Pro, Lerd ve EnvKit'in üçü de aynı şeyi satıyor ve F-1 üçünün de en çok
anılan özelliği. Container içinde bir toplayıcı gerektirdiği için P0 değil.

### G — Veritabanları

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| G-1 | Zamanlanmış yedek | ✅ | — |
| G-2 | Adlandırılmış snapshot | ✅ | — |
| G-3 | Masaüstü DB istemcisini bağlı açma | 🟡 | `connect.rs` dizeyi veriyor ve kopyalatıyor; istemciyi **açan** yok (`apps.rs`'te tableplus/dbeaver sıfır isabet) |
| G-4 | Servisler arası taşıma / sürüm göçü | ⬜ | — |

### H — Üretim köprüsü

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| H-1 | Registry push, deploy reçeteleri, sağlayıcıdan pull | ⬜ | `release.rs`'te `deploy` yalnızca yorumda geçiyor |

Zor yarısı bitti: soyu geliştirme imajı olan üretim imajı, çalıştırılıp sorularak
temiz olduğu kanıtlanmış (`.env` yok, Xdebug yok). Sahada Laradock dışında
kimsede yok. Kolay yarısı — push ve reçete — eksik.

### I — Performans: Docker eleştirilerinin doğru olanı

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| I-1 | Bind-mount performans katmanı | ⬜ | `mutagen` / `:cached` / `delegated` sıfır isabet |
| I-2 | Boştaki projeyi askıya alma | ⬜ | — |

**Listedeki en sonuç doğurucu madde.** Diğer her boşluk bir özelliğe mal oluyor;
bu *argümana* mal oluyor: macOS ve Windows'ta bind-mount edilmiş kaynak kod,
insanların Docker tabanlı bir iş akışını bırakmasının en yaygın tek nedeni. DDEV
Mutagen'i paketleyip varsayılan açıyor. Burada Herd'dekinin 4 katı süren bir test
suite'ine "tekrarlanabilirlik" cevap değildir.

### J — Runtime'lar

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| J-1 | Bun, Deno | ⬜ | `project.schema.json`'da yok |
| J-2 | Corepack / paket yöneticisi sabitleme | ⬜ | node şablonu npm çalıştırıyor |

Altı runtime, PHP 5.6–8.5, Node 16–23 ve tespit anında okunan
`.nvmrc`/`engines.node` — bu satır rekabetçi.

### K — AI katmanı

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| K-1 | Agent config yükleyicisi | ✅ | — |
| K-1 | Codex (TOML), Zed | ⬜ | bilerek dışarıda, gerekçesi §1'de |
| K-2 | Container içinde agent farkındalığı | ⬜ | — |

**Hiçbir rakip MCP yüzeyini kontrol edilen bir kontrattan türetmiyor** — üç
kontrat testi her aracı `contracts/ipc.json`'a çapraz kontrol ediyor, var olmayan
bir komutu adlandıran araç build'i kırıyor. Bu gerçek bir farklılaştırıcı.

### L — Onboarding

| # | Madde | Durum |
| --- | --- | :-: |
| L | XAMPP, Laragon | ✅ |
| L | MAMP, Sail, Valet | ⬜ |

### M — Küçük maddeler, her biri ucuz

| # | Madde | Durum |
| --- | --- | :-: |
| M-1 | Proje grupları / favoriler | ⬜ |
| M-2 | Mail *gönderme* / relay | ⬜ |
| M-3 | Paylaşım URL'sinde QR kod | ⬜ |
| M-4 | Her siteyi listeleyen açılış sayfası | ⬜ |
| M-5 | Proje başına ortam değişkenleri | ⬜ |
| M-6 | Proje başına dizin listeleme anahtarı | ⬜ |
| M-7 | Arayüz dilleri (şu an 2) | ⬜ |
| M-8 | Alternatif yüzeyler (TUI, tray-only, PWA) | ⬜ |
| M-9 | Framework geçiş komutları (`ddev drush`) | ⬜ |
| M-10 | SSH agent'ının container'a iletilmesi | ⬜ |
| M-11 | Stripe webhook dinleyicisi | ⬜ |
| M-12 | `.loc` için OAuth callback yönlendirme | ⬜ |

M-7 artık bir kod değişikliği değil: tray ve menü etiketleri `tray_relabel`
üzerinden frontend'den besleniyor, yani üçüncü dil bir locale dosyası.

### N — Sahada yalnız Lerd'de olan

| # | Madde | Durum |
| --- | --- | :-: |
| N | Worktree başına ortam | ⬜ |

`git worktree add` dala kendi subdomain'ini, kendi veritabanını, kendi
`.env`'ini veriyor. Container tabanlı bir araç için Podman tabanlı olandan
*daha* doğal — dal başına veritabanı bir volume adı, dal başına yönlendirme bir
Traefik kuralı. Kimsenin hızlıca kopyalayamayacağı tek özellik istenirse aday bu.

### Önde olan ve önde kalması gereken satırlar

`sysinfo` ile gerçek host metrikleri; bayt bayt doğrulanmış generator; gözden
geçirilmiş yetkili hosts yazımı; geliştirme imajından türeyen üretim imajı;
container **ve** host PTY; yalnızca Laradock'un eşleştiği ağır servis kataloğu —
Laradock'un ise hiç GUI'si yok; **28 iskelet şablonu, her kurucusu gerçek bir
container'da ölçülmüş** (Herd `laravel new`'e dayanıyor, Laragon'un Quick app'inde
dört giriş var); ve tek bir ortak config şekliyle altı runtime — FlyEnv 13,
ServBay 8 iddia ediyor ama ikisi de host binary'si yönetiyor, yani sonsuza kadar
taşıdıkları bir paketleme yükü; StackVo'nunki bir şablon.

### Girilmeyecek kavgalar

- **Native-binary hız savaşı.** FlyEnv "<100 ms açılış", Laragon "~10 MB RAM"
  yayınlıyor. Kazanılamaz. Ama I-1'in ayrımı: *soğuk açılış* kaybedilen bir
  tartışma, *dosya G/Ç* gerçek bir kusur — birincisi ikincisini görmezden
  gelmenin bahanesi olmasın.
- **LLM sağlayıcı proxy'si** (ServBay'in AI Gateway'i). Kapsam dışı. Yerel AI
  *servisleri* farklı bir soru — §5.
- **FlyEnv'in 50+ aracı** (base64, QR, regex test ediciler). Odaksız.
- **Portable mod.** Docker bağımlılığıyla anlamsız.
- **Laradock'un 130 servisinin peşine düşmek.** Genişliğin kendisi için
  genişlik, bir kataloğun bakımsız hâle gelme yolu.
- **Ücretli katman.** Herd $99/yıl, ServBay $59/yıl, Laragon ticarileşip
  fork'landı. EnvKit, ForgeKit ve DDEV tam oradan saldırıyor; MIT o çizginin
  doğru tarafı.

---

## 3. Mühendislik borcu — kalan

Ürünün ne yapamadığı değil, **mühendisliğin** ne taşıyamadığı: aynı kod tabanı
2100 commit, on geliştirici ve bir kurumun 300 makinesinde olduğunda ilk
kırılacak yerler.

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| 2 | Güncelleme endpoint'i | ⛔ | `latest.json` → HTTP 404; repo yok. Sahiplik kararı |
| 10 | `tauri-specta` ile tip üretimi | ⬜ | `specta`/`ts-rs`/`typeshare` bağımlılıkta yok |
| 12 | E2E (`tauri-driver`) | ⬜ | driver/wdio/playwright yok, CI'da e2e job'ı yok |
| 21 | Sürüm kanalları, kademeli dağıtım, geri alma | ⛔ | `tauri.conf.json`'da `channel`/`rollout`/`paused` yok; #2'nin arkasında |
| 22 | Platform kapsamı (Linux aarch64, Win ARM64) | ⬜ | dört hedef |
| 24 | RTL | 🟡 | bağ test edilmiş; `vuetify.js`/`i18n` içinde `rtl` yapılandırması yok |
| 25 | Erişilebilirlik beyanı (VPAT / EN 301 549) | ⬜ | #12 olmadan üretilemez |
| 27 | `list_projects` cache | 🟡 | gizli pencerede yavaşlama kapandı; cache yok |
| 31 | Air-gapped kurulum | 🟡 | gidiş-dönüş tam ve arayüzde; paket yolu yok |
| 33 | Sözleşme kapısının harici bağımlılığı | 🟡 | checkout var ama **suite A hiç koşmuyor** — bu makinede de `NO_MANIFESTS` |
| 34 | Web sürümü / HTTP ikilisi | ⬜ | `src/bin/` yalnız `stackvo-mcp.rs` |
| 35 | Windows ve Linux dallarının çalıştırılması | ⬜ | CI üç OS'ta koşuyor; ayrıcalık yolları koşmadı |

Kapananlar (kayıt için): panic hook + crash dosyası, SECURITY.md'nin ölü linki,
README'nin iki yanlış sayısı, kapsam ölçümü, sürüm eşitlik testi, macOS imzasız
build uyarısı, `elevate` quoting'i, sistem proxy'si, `ProgressSink`, bozuk
tercih dosyasının yedeklenmesi, `Settings.vue`/`ProjectDetail.vue`'nun
bölünmesi, ARCHITECTURE.md, merkezî politika katmanı, private registry ön eki,
Docker karar katmanı, keystore ile sır yönetimi, denetim izi, `stats_history`
kalıcılığı, mutex poisoning, performans bütçesi, gömülü PTY'nin arayüze
bağlanması, tray etiketlerinin frontend'den beslenmesi.

**Teşhis, ve hâlâ geçerli:** bu, tek bir çok iyi mühendisin yazabileceği en iyi
kod tabanlarından biri — ve tam olarak o yüzden kurumsal değil. Eksikler kod
kalitesinde değil, **kalitenin kod dışına, otomatik ve devredilebilir hâle
çıkarılmasında**. Bugün 1 yazar var; ikinci geliştirici geldiği gün ya da altıncı
ayda hafıza soluklaştığında çalışmayacak olan şey bu.

---

## 4. Önerilen sıra

Karar gerektirmeyenler arasından, etki ÷ efor ile.

1. **I-1 bind-mount ölçümü.** En sonuç doğurucu madde. İlk teslimat **ölçümün
   kendisi** olmalı: macOS'ta gerçek bir Laravel test suite'ine karşı düz bind
   vs `:cached`/`delegated` vs bir senkron katman. Bunun bir mount bayrağı mı
   yoksa Mutagen sınıfı bir alt sistem mi olduğuna o ölçüm karar verir.
2. **G-3 masaüstü istemciyi açma.** Yarım: dize var, açan yok. `apps.rs` zaten
   kurulu uygulamaları bulan modül, `connect.rs` zaten doğru dizeyi üretiyor —
   aradaki tek şey bir `open`.
3. **E-3 LAN paylaşımı.** `sslip.io` bir alan adı biçimi; sertifika ve
   yönlendirici tarafı E-2 ile zaten çoklu isim öğrendi.
4. **J-1 / J-2 Bun, Deno, corepack.** Altı runtime'ın paylaştığı `LangConfig`
   şablonu var; yeni mekanizma gerekmiyor.
5. **D-1'in kalanı: Solr, ClickHouse.** Aynı şablon işi, aynı dokuz dokunma
   noktası (§1, D-1).
6. **#12 E2E.** #25'in ön koşulu ve `commands.rs`'in %18 kapsamının önündeki tek
   gerçek engel.

**F bölümü** en büyük ürün boşluğu olarak duruyor ve container içinde bir
toplayıcı gerektirdiği için ayrı bir tur. **N (worktree başına ortam)** sahayla
eşitlemek yerine önüne geçirecek tek madde, ve taban sağlamlaşınca.

---

## 5. Karar bekleyenler

Kodla çözülmeyen maddeler. Cevaplanmadan planlanamazlar — sessizce varsayılan
seçmek, bu listenin var olma sebebine aykırı.

1. **Kullanıcı uzatma noktaları (C-1, C-2, B-4).** `quickcmd.rs`, webview'in asla
   çalıştırılacak bir programı adlandıramayacağını savunuyor ve o gerekçe
   sağlam. Ama o gerekçe *webview*'in seçmesine karşı; *workspace*'in diske
   yazılmış bir dosyayla beyan etmesine karşı değil. Bir çalışma alanı kendi
   servis şablonunu ve compose overlay'ini beyan edebilir mi? Cevap üç maddeyi
   birden karara bağlıyor.
2. **İkinci bir arayüz (A-1).** Bir CLI, sözleşmeyle senkron tutulacak üçüncü
   yüzey demek. E ve F suite'leri tam da bu kaymayı durdurmak için var ve MCP
   sunucusu desenin genişlediğini kanıtladı — ama sonradan değil, önceden
   onaylanmaya değer.
3. **Yerel AI servisleri (D-1).** **Ertelendi** olarak kayıtlı, kapsam dışı
   değil. Ollama, Qdrant ve pgvector birer katalog servisi olsun mu — kapatılan
   LLM-gateway sorusundan farklı bir soru.
4. **Güncelleme endpoint'i ve imzalama secret'ları (#2).** `latest.json` nerede
   yayınlanacak: `stackvo/stackvo` release'leri mi, yeni bir repo mu? Özel
   anahtar `~/.tauri/stackvo.key`'de duruyor ve repository secret'ı olarak
   eklenmesi gerekiyor; Apple/Windows secret'ları ücretli hesaplara bağlı. #21
   bunun arkasında bekliyor.
5. **Kapsam eşiği.** Ölçüm var, kapı yok. %61.60'ı mı yoksa daha düşük bir tabanı
   mı kilitleyeceği mühendislik değil, politika kararı.

---

## 6. Kararlar

Numaralandırılmış, çünkü sonraki bir karar öncekinin üstüne yazabilsin —
bir kod yorumunun sahip olamayacağı özellik bu. Koddaki "ADR 0005" atıfları bu
tabloyu kastediyor.

### 0001 — Domain bandı Tauri'yi bilmez

- **Status:** accepted
- **Decision:** `commands.rs` Tauri tipi adlandıran tek modül. Altındaki her şey
  gerçekten ihtiyaç duyduğunu alır: `State` yerine `&Path`, handle yerine
  `&dyn ProgressSink`. Bir komutun işi Tauri şeklindeki dünyayı düz argümanlara
  açmak, tek bir domain fonksiyonu çağırmak ve sonucu geri şekillendirmek.
- **Consequences:** Kural bir yorum değil bir test —
  `architecture_claims.rs::only_the_command_layer_names_a_tauri_handle`.
  MCP sunucusu ve gelecekteki her tüketici aynı çekirdeğe ulaşır.

### 0002 — Üretilen dosyalar render edilir, düzenlenmez

- **Status:** accepted
- **Decision:** `generated/` altındaki her şey ve proje başına üretilen her dosya,
  manifest ve `.env`'den **her seferinde bütün olarak** render edilir. Hiçbir şey
  yamalanmaz. `generated/` her an silinip yeniden kurulabilir. Kullanıcının
  düzenlemesi gereken tek dosya `stackvo.json` ve şeması
  `additionalProperties: false`.
- **Consequences:** Bir ayar şemada yoksa manifest anahtarı olarak
  kaçırılamaz. Sırların `generated/` içinde kalması ADR 0010'un kabul ettiği
  sınırın sebebi.

### 0003 — Konu başına tek işlem, arka uçta zorlanır

- **Status:** accepted
- **Decision:** Gerçek arka uçta. `AppState::inflight` işlem yürüyen konuların
  kaydı. **İki problem, iki farklı cevap:** kullanıcı başlattığı bir işlem meşgul
  bir konuya çarparsa **anında başarısız olur** (bir çift tıklama, bayat bir
  düğme — kuyruğa almak birini bir dakika sonra unuttuğu bir eylemle şaşırtır);
  üretim ise pek çok işlemin iç adımı ve paylaşılan dosyalar yazıyor, o yüzden
  **sıraya girer**.
- **Consequences:** Ön yüzdeki meşgul bayrağı tek bir görünümün fikri; tray, ikinci
  pencere ve kısayol aynı komutlara ulaşıyor ve hiçbiri diğerinin bayrağını
  göremiyor.

### 0004 — Hatalar dize değil, katalogdan hint taşıyan kodlar

- **Status:** accepted
- **Decision:** Tek şekil:
  `StackvoError { code, message, hint, hint_key, details }`. `code` dallanılan
  şey; zarf yok, `Ok(T)` doğrudan payload. `hint_key`
  `src-tauri/src/hints.rs`'teki bir girdiyi adlandırıyor, böylece ön yüz
  **çevrilmiş** bir öneri gösterirken log, crash raporu ve MCP yüzeyi İngilizceyi
  alıyor.
- **Consequences:** Selefi HTTP 200 ile `{ success: false }` dönüyordu — bir hata
  `.success` okunana kadar başarı gibi görünüyordu, ve dallanmanın tek yolu
  metnini eşleştirmekti.

### 0005 — Uzun işlemler bir sink üzerinden rapor verir

- **Status:** accepted
- **Decision:** İki kural. **~2 saniyeyi aşabilen hiçbir şey bloke etmez** —
  hemen bir `OperationId` döner ve olaylarla rapor verir. **İlerleme bir handle
  değil bir trait üzerinden gider:** `ProgressSink`. Masaüstü `Sink::App`, MCP
  `Null`, testler `Recording` veriyor.
- **Consequences:** `run_operation` — her uzun işlemin geçtiği huni — ilk kez
  test edilebildi (%98 kapsam). Selefi bir HTTP isteğini bloke edip nginx proxy
  timeout'unu 600 saniyeye çıkarmıştı.

### 0006 — IPC sözleşmesi yazılır, üretilmez

- **Status:** accepted, bilinen bir haleti var
- **Decision:** Elle yazılmış sözleşme şimdilik kalıyor ve **kayma imkânsız değil,
  gürültülü** yapılıyor. `tauri-specta` ölçüldü ve ertelendi: 144 komutun
  tamamının nasıl bildirildiğini değiştiriyor ve bunu başka bir işin ortasında
  yapmak diğer her değişikliği gözden geçirilemez kılardı. `contract_agreement.rs`
  sözleşme ↔ implementasyon ↔ kayıt üçlüsü ayrıştığında build'i kırıyor.
- **Consequences:** Ön yüz tipsiz kalıyor (§3, #10). Kaymayı bir derleyici değil
  bir test tutuyor — ama tutuyor: bugün sıfır drift.

### 0007 — Tam olarak bir ayrıcalıklı çağrı

- **Status:** accepted
- **Decision:** **Pencereli bir uygulama, bir alt sürecin parola sormasına asla
  izin vermemeli.** Yükseltme tek modülde, `elevate.rs`, platformun pencereli bir
  uygulamaya verdiği mekanizmayla: `osascript`'in `with administrator
  privileges`'ı. Script sabit, yollar `argv` ile gidiyor — interpolasyon yok.
- **Consequences:** `mkcert -install` gibi kendi parola isteyen araçlar, terminali
  olmayan bir uygulamada sessizce takılırdı. `/etc/hosts` yazımı ve sertifika
  güveni tek kapıdan geçiyor ve ikisi de denetim izine düşüyor.

### 0008 — Kırıcı bir sözleşme değişikliği nedir

- **Status:** accepted
- **Decision:** **Sürüm, bir çağıranın fark edeceği şeyi tarif eder, başka hiçbir
  şeyi.** Major: bir komut/olay/tip kaldırılır ya da adı değişir; `kind` veya
  `returns` değişir; bir argüman kaldırılır, adı değişir, tipi değişir; **zorunlu**
  bir argüman eklenir; bir komut bildirdiği olayı yaymayı bırakır; bir olay
  payload'ından ya da adlandırılmış tipten alan kalkar; `status` `deferred` olur.
  Minor: ekleme, **isteğe bağlı** argüman, alan ekleme, `deferred`'ın
  cevaplanabilir olması. Değişmez: `why`, `notes` — **düzyazı yüzey değildir**.
- **Consequences:** Sayı türetilebilir hâle geldi; herkes diff'ten yeniden
  kurabiliyor. ADR 0006'nın güvene bırakılmış yarısını kapattı: adlandırılmış
  tipler artık alan alan kilide karşı karşılaştırılıyor.

### 0009 — Bir politika dosyası kilit değildir

- **Status:** accepted
- **Decision:** Bir **iş birliği mekanizması**, güvenlik sınırı değil — beş
  yerde birebir aynı cümleyle, İngilizcesiyle: **not a security boundary**.
  (`policy.rs`, `contracts/ipc.json`, `PolicyNotice.vue`, `en.js` ve burası;
  `policy_claims.rs` beşini birden tutuyor, çünkü dördünün söyleyip birinin
  susması tam olarak birinin ona göre plan yaptığı hâldir.) Uygulama, normal yapılandırılmış bir makinede kullanıcının
  kendi hesabının çoğu zaman yazabildiği bir JSON okuyor;
  `STACKVO_POLICY_FILE` onu herhangi bir yere yönlendirebiliyor. İkisi de doğru
  ve ikisi de yamalanacak bir kusur olarak görülmüyor. **Anahtarı üzerine bantlanmış
  bir kilit satmak, hiç kilit satmamaktan kötüdür** — çünkü biri ona göre plan
  yapar. Üç yol okunuyor:
  `/Library/Managed Preferences/com.stackvo.desktop.json` (macOS),
  `%ProgramData%\StackVo\policy.json` (Windows), `/etc/stackvo/policy.json`
  (Linux).
- **Consequences:** Katman atlatılabilir ve dokümantasyon bunu tarif ettiği
  nefeste söylüyor. Gerçek bir sınıra ihtiyacı olan kuruluşun ihtiyacı cihaz
  yönetimi, bu değil. Politika süreç başına bir kez okunuyor; bir değişiklik
  yeniden başlatma gerektiriyor.

### 0010 — Sırlar `.env`'den çıkar, diskten değil

- **Status:** accepted
- **Decision:** Bir kimlik bilgisi `.env`'den OS keystore'una taşınıyor ve yerine
  `keychain:<entry>` referansı kalıyor — ama **değer hâlâ
  `generated/docker-compose.dynamic.yml`'a render ediliyor** ve modül yorumu,
  sözleşme girdisi, `PRIVACY.md` ve Settings paneli bunu söylüyor. `.env` elle
  bakılan, destek başlıklarına yapıştırılan, senkronlanan ve yedeklenen dosya;
  `generated/` ise ADR 0002'ye göre her koşuda sıfırdan yazılan çıktı. Birinciden
  ikinciye taşımak **gerçek ve kısmi** bir azaltma.
- **Consequences:** Bash CLI taşınmış bir anahtarı okuyamıyor ve hiçbir şey bunu
  değiştiremez; `doctor` her ikisini de tutan bir çalışma alanını rapor ediyor.
  macOS ve Windows'ta bir yeni crate, Linux'ta on dört, kilitte yirmi dokuz.
  `generated/`'dan da çıkarmak bir v2 değişikliği ve burada yarım bırakılmadı.

---

## 7. Ölçüm

Mekanik olarak sayılabilenler koda karşı tutuluyor:
`src-tauri/tests/platform_matrix_claims.rs` yanlış bir sayıda build'i kırıyor.

| | Sayı | Nasıl sayıldı |
|---|---|---|
| Toplam IPC komutu | **168** | `contracts/ipc.json` → `commands` (165 Rust + 3 `frontend-plugin`) |
| Bunlardan `#[tauri::command]` olarak yazılmış | **164** | `commands.rs`, `#[cfg(test)]` dışı |
| Frontend kaynak dosyası | **103** | `src/**/*.{js,vue}`, spec dosyaları hariç |
| Bunlardan `@tauri-apps` kullanan | **16** | aynı küme içinde metin taraması |
| **Veri katmanının geçtiği fonksiyon** | **1** (`src/lib/ipc.js` → `call()`) | `invoke(` `ipc.js` dışında **0** yerde geçiyor |
| `ipc.js` sarmalayıcısı | **161** | `api` nesnesinin üye sayısı |
| Rust kaynağı | **64 modül, 45.464 satır** | `src-tauri/src/*.rs` |

Elle sınıflandırma, kapıya dahil değil — yöntemi yazılı ki bir sonraki okuyucu
yeniden üretebilsin:

| | Sayı | Yöntem |
|---|---|---|
| Docker'a bollard (API) ile giden komut | 15 | gövdesinde `engine::` çağrısı |
| Docker'a `docker compose` (CLI) ile giden komut | 14 | gövdesinde `runner::` / `compose_*` |
| Host dosya sistemine dokunan komut | 34 | `std::fs`, `workspace::`, `scaffold::`, `config::Env`, `env_writer::` |
| Ayrıcalık (parola) gerektiren komut | 6 | `elevate::` ya da hosts yazan yol |

Veri yolunun tek fonksiyondan geçmesi, bir web sürümü sorulduğunda (§3, #34) en
önemli tek bulgu: `call()`'un gövdesi değişirse kalan dosyalar değişmez, ve
`invoke(` kelimesinin `ipc.js` dışında sıfır yerde geçtiği her koşuda
doğrulanıyor. Akışlar (log, stats, events) IPC olayı yerine SSE ya da
WebSocket'e taşınır — bu bir taşıyıcı değişikliği, yetenek kaybı değil.

**Bir web sürümünde karşılığı olmayan dört komut**, çünkü hepsi pencerenin ya da
masaüstünün kendisi hakkında: `tray_relabel` (tepsi menüsü),
`window_close_action` (pencere kapatma davranışı), `updater_status` ve
`updates_check` (uygulamanın kendini güncellemesi). Docker tarafında böyle bir
kayıp yok — bollard bir HTTP istemcisi ve sunucu host'ta çalıştığı sürece fark
etmiyor; ayrım Docker'da değil, **sunucunun nerede çalıştığında**.

---

## 8. Bu dosya nasıl doğru kalır

Üç kural, ve ikisinin arkasında kapı var:

1. **§5'teki karar tablosu ve §7'deki ölçüm testlerle tutuluyor.** Bir karar
   Status/Decision/Consequences taşımazsa, ya da bir sayı ağaçla uyuşmazsa,
   build kırılır.
2. **§2–§4 kapıya bağlanamaz** — "yapılmadı" ölçülemez. Elde olan tek şey her
   satırın **nasıl bakıldığını** taşıması; bir sonraki oturum tabloyu okumak
   yerine aynı kontrolü tekrarlayabilir.
3. **Bir madde ancak §1'e bir kayıt bırakarak §2'den çıkar** — kararı ve yolda
   bulunan hatayı yazarak. Bir sonraki okuyucunun ihtiyaç duyduğu şey ne
   yapıldığı değil, neden öyle yapıldığı.
