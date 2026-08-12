# StackVo — durum, kararlar ve kalan işler

**Son ölçüm: 11 Ağustos 2026.** `docs/` altındaki iki dokümandan biri budur;
diğeri [`servis-market-mimarisi.md`](servis-market-mimarisi.md), C-1, C-2 ve
D-2'nin nasıl kapanacağını anlatan bir tasarım raporu ve tarif ettiği iş
bitince silinecek.

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

### S — servis paketleri ve market

Servisler binary'den çıkıp `stackvo/stackvo-service-packages`'a taşındı, ve bir
servisin birden çok sürümü aynı anda çalışabilir hâle geldi. Tasarımın tamamı
[`servis-market-mimarisi.md`](servis-market-mimarisi.md)'de; burada yalnız
**neyin bittiği ve neyin bitmediği** duruyor.

**Faz 4 ve 5 kapandı, Faz 6 yarım.** Ağ kaynağı, hava boşluğu politikası, ilk
açılış kapısı, göçün yedeği ve arayüzü, healthcheck'ler ve örnek başına alt alan
adı indi. Geriye tek büyük madde kaldı: **gömülü şablonların silinmesi (S-16)**,
ve o bir mühendislik işi olmaktan çok bir karar bekliyor — §4'ün sonunda.

**Doğrulandı, akıl yürütülmedi.** `examples/side_by_side.rs` bu makinede iki
mysqld başlattı: **8.0.46 ve 9.4.0**, ayrı volume, ayrı port (3316/3326 — 3306'yı
makinenin kendi MySQL'i tutuyordu), ve `stackvo-mysql` ağ içinde çözülüyor.

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| S-1 | Paket formatı: üç şema + compose politikası | ✅ | `contracts/package*.json`, `registry.schema.json`, `compose-policy.json`; `tests/package_contract.rs` şema ile `pkg::Manifest`'i karşılaştırıyor |
| S-2 | 25 servis, 101 sürüm paket olarak | ✅ | `stackvo-service-packages/packages/`; `examples/build_packages.rs` üretti, her manifest `pkg::parse`'dan geçti |
| S-3 | Paket deposunun kendi CI'ı | ✅ | altı araç, iki iş; 105 fragment `docker compose config` + politika kapısından geçiyor |
| S-4 | Örnek modeli (`instances.json`) | ✅ | `instances.rs` + `ports.rs`; çakışan port/volume/alias reddediliyor |
| S-5 | `.env` → `instances.json` göçü | ✅ | `handover.rs`; `tests/handover_equivalence.rs` image/port/volume/environment'ı alan alan tutuyor |
| S-6 | Render hattının takası | ✅ | tablo yoksa eski yol **bayt bayt**, varsa yeni yol, geri düşme yok |
| S-7 | Çoklu sürüm gerçekten çalışıyor | ✅ | `examples/side_by_side.rs`, yukarıdaki ölçüm |
| S-8 | Compose politikası istemcide de | ✅ | `compose_policy.rs`; beş saldırının beşi reddediliyor |
| S-9 | Market + örnek arayüzü | ✅ | 16 IPC komutu, `Market.vue`, `useMarket.js` |
| S-10 | Market: yerel kaynak | ✅ | `market.rs`; hash zinciri, sequence geri gitme reddi, atomik kurulum |
| S-11 | **Paketlerde healthcheck** | ✅ | 101 paketin **98'i** bildiriyor, kalan üçü blackfire ve gerekçesi `validate.mjs`'in `HEALTH_EXEMPT` tablosunda; `render.rs` manifestten yazıyor, fragment yazamıyor; `examples/health_probe.rs` **24/24**'ünü gerçek konteynerde yeşil ölçtü |
| S-12 | Market: ağ kaynağı (HTTPS) | ✅ | `market::HttpSource`; `http://` reddi, ETag, 8 MiB gövde sınırı, sistem proxy'si |
| S-13 | İmza doğrulaması | ⛔ | anahtar yok; `Trust::Signed` **reddediyor**. §5 madde 4 açık karar |
| S-14 | Hava boşluklu paket (`offlineBundle`) | ✅ | `policy::Market`; yedi anahtar okunuyor, kurulum yolunda zorlanıyor, `policy_status` gösteriyor |
| S-15 | Ağ kapısı (hiç katalog çekmemiş makine) | ✅ | `CatalogueGate.vue`, `workspace.catalogueFetched`; "internet yok" ile "burada katalog yok" ayrı iki cümle, atlanabilir |
| S-16 | Gömülü şablonların silinmesi | 🟡 | takas indi, `skeleton/core/templates/services/` **duruyor** — tablosuz çalışma alanları hâlâ onu kullanıyor. Kalanı §4'te |
| S-17 | Göçün `.env` yedeği ve arayüzü | ✅ | `handover::apply` önce `.env.pre-market.bak` yazıyor ve servis satırlarını **tam satır** yorumla işaretliyor; `handover_preview`/`handover_apply` + Market sayfasında panel |
| S-18 | Örnek başına alt alan adı | ✅ | `Instance::domain`; birincil çıplak adı korur, ötekiler `phpmyadmin-5-2.stackvo.loc`. 24 paket artık `instancing.multiple: true` |

**Yolda bulunan üç hata**, üçü de yalnız çalıştırınca görünen türden:

`template.rs` **ASCII olmayan her karakteri çift kodluyordu** (`u8 as char` bir
Latin-1 çözümü). Golden fixture bunu yakalayamazdı — karşılaştırmanın iki
tarafı da bozuk çıktıydı, yani dosya kendisiyle uyuşuyordu.

`postgres.conf` ve `elasticsearch.yml` her generate'te yazılıp **hiçbir şablon
tarafından mount edilmiyordu**; içlerindeki her ayar ölüydü. Paketlerde
düzeltildi ve çalışan konteynerlerde doğrulandı (postgres `max_connections=200`,
ES `thread_pool.write.queue_size=1000` — ikisi de yalnız o dosyalarda var).

**MySQL 9.x hiç açılmıyordu** — `contracts/CONFLICTS.md` **C-21**. Sürüm
seçicisi 9.7 ve 9.4 sunuyor, tek bir `my.cnf` her sürüme mount ediliyor, ve iki
direktifi MySQL 9 kaldırmış. Bir sürüm bir dizin kuralının var olma sebebi.

**Ölçüm de bir hata buldu:** `tools/eol.mjs` 101 sürümün **20'sinin**
`supported` dediği hâlde end-of-life olduğunu gösterdi — üçü uygulamanın kendi
önerdiği sürüm (mysql@8.0, mariadb@10.6, redis@7.0).

#### İkinci tur: healthcheck'i ölçmek üç hata daha buldu

Tablo yazmak ucuzdu; **çalıştırmak** pahalıydı ve bulan oydu.
`examples/health_probe.rs` her servisi kurup kaldırıyor ve motorun kendi sağlık
durumunu soruyor. Katalogda bildirilen 24 healthcheck'in 24'ü yeşil — ama ilk
turda 22'si yeşildi.

**Kafka hiç açılmıyordu, ve hiç açılmamıştı.** Konteyner sonsuz yeniden
başlıyordu: `cp-kafka` `appuser` olarak koşuyor, Docker var olmayan bir bind
hedefini root'a ait yaratıyor, ve giriş noktası
`-Xlog:gc*:file=/var/log/kafka/kafkaServer-gc.log` veriyor. Yazamayan **JVM**
başlamıyor, broker değil. Aynı mount paket öncesi şablonda da vardı, yani bu bir
gerileme değil: Kafka hiç çalışmadı ve kimse söylemedi, çünkü kimse *ayakta mı*
diye sormamıştı. Mount kaldırıldı; imajın kendi `/var/log/kafka`'sı zaten
`appuser`'ın, ve hiç yazılmamış bir dosyayı kimse kaybetmiyor.

**Broker Zookeeper'ını bulamıyordu.** Eski şablon iki konteyneri tek dosyaya
koyuyor ve `KAFKA_ZOOKEEPER_CONNECT: zookeeper:2181` yazıyordu — çözülüyordu,
çünkü `zookeeper` compose servis anahtarıydı ve Compose her anahtara bir DNS adı
verir. Örnek başına anahtar `kafka-7-5-0-zookeeper`, yani o düz metin hiçbir şeye
çözülmüyor. Ad tek yerde türetiliyor ve fragment artık
`{{ companion.zookeeper.host }}` ile onu istiyor.

**Zookeeper'ın `ruok`'u kapalı.** Dört harfli komutlar ZooKeeper 3.5'ten beri
varsayılan olarak kapalı ve `ZOOKEEPER_4LW_COMMANDS_WHITELIST=srvr,ruok`
**açmadı** — iki yönlü de bu makinede ölçüldü. Yani zarif olan kontrol, hiçbir
işe yaramayan bir ortam değişkeninin yanında hiç geçmeyen bir kontrol olacaktı.
Companion'ın broker için olması gereken şey istemci portunda erişilebilir olmak,
o yüzden sorulan bu.

Ve **mongo-express** 401 döndürüyordu: basic auth varsayılan açık, kimlik
bilgileri birer ayar, ve manifestin `health` bloğu ayar okuyamıyor (fragment gibi
substitute edilmiyor, uygulama yazıyor). Dürüstçe sorulabilen daha zayıf soru
soruluyor: sunucu dinliyor mu.

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
| C-1 | Kullanıcının kendi servis şablonu | 🟡 | Paket formatı ve `pkg::Tree` mekanizmanın tamamını veriyor (S-1, S-2); **kullanıcının kendi kaynağını göstermesi** var, kendi paketini yazması için bir yüzey yok |
| C-2 | Kullanıcının kendi compose servisi | 🟡 | Bir paketin compose fragment'i artık kullanıcının yazabileceği bir şey ve `compose_policy` ne diyebileceğini söylüyor; üçüncü taraf kaynak politikası yok (S-14) |

DDEV'in kayıt defteri (`addons.ddev.com`), 36 resmî ve 100+ topluluk eklentisi
var. Container tabanlı bir araç için kullanıcının kendi compose dosyasını
reddetmek, container tabanlı olmayı alternatifinden *daha kötü* yapan tek şey.

### D — Servis kataloğu

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| D-1 | Nesne depolama, arama | ✅ | — |
| D-1 | Solr, ClickHouse | ⬜ | şablon dizini yok |
| D-1 | Ollama, Qdrant, pgvector | 🔒 | §5'te **ertelendi** olarak kayıtlı, kapsam dışı değil |
| D-2 | Aynı servisten birden çok örnek | ✅ | `instances.json`; bu makinede 8.0.46 ve 9.4.0 yan yana çalıştırıldı (S-7) |

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

### S-16'nın önündeki şey bir karar, kod değil

Gömülü şablonları silmek, `render_generated`'ın `.env` dalını silmek demek — ve
o dal bugün var olan **her** çalışma alanının çalışma sebebi. Silindiği anda
göç etmemiş bir kurulum servislerini başlatamaz hâle gelir.

Göç artık mümkün (S-17: yedek, işaretleme, önizleme ve düğme) ve katalog artık
gelebilir (S-12, S-15). Eksik olan tek şey, göçü **reddeden** bir kullanıcıya ne
olacağı. Üç cevap var ve üçü farklı ürünler:

1. **Zorunlu göç.** Yeni sürüm açılışta göçü dayatır; reddeden yığınını
   çalıştıramaz. En temiz kod, en sert davranış.
2. **Bir sürüm boyunca ikisi.** `.env` dalı kalır ama bir uyarı taşır ve
   sürüm notu tarihi verir. Kod iki yol taşımaya devam eder — tam olarak
   Faz 6'nın bitirmek istediği şey.
3. **Sessiz göç.** Uygulama açılışta kendi göç eder. Yedek var, ama bir
   kullanıcının servis tanımlarını sormadan değiştirmek §5'in cinsinden bir
   karar.

Bu, `docs/durum.md` §5'e ait bir soru ve orada altıncı madde olarak duruyor.
Cevaplanmadan S-16'ya başlamak, üç davranıştan birini sessizce seçmek olur.

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
6. **Göç etmeyi reddeden çalışma alanı (S-16).** Gömülü şablonlar silindiğinde
   `.env`'den render eden dal da gider, ve bugün var olan her kurulum o dalda.
   Göç mümkün ve geri alınabilir; soru, reddedene ne olacağı: zorunlu göç, bir
   sürüm boyunca iki yol, yoksa açılışta sessiz göç. §4'ün sonunda üçünün de
   bedeli yazılı. Bu cevaplanmadan S-16 bir tercihi sessizce seçer.

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

### 0011 — Uygulama hiçbir servis tanımı taşımaz

- **Status:** accepted
- **Decision:** `skeleton/core/templates/services/` binary'den tamamen çıkıyor
  ve yerine gömülü bir katalog anlık görüntüsü **konmuyor**. Ağı olmayan bir
  makinede ilk açılışta market boş görünür ve "ağ gerekli" der. Ara çözüm —
  imzalı bir `registry.json`'ı gömmek — reddedildi: gömülü her bayt bir sonraki
  sürüme kadar bayatlar, ve "gömülü olan yalnızca liste" ayrımı altı ay sonra
  kimsenin hatırlamayacağı bir ayrımdır. Tek kural olarak "servis tanımı
  binary'de yoktur" savunulabilir; "neredeyse yoktur" savunulamaz.
- **Consequences:** İlk açılış bir ağ kapısı kazanıyor — `RequirementsGate` ve
  `BootstrapGate` deseninin üçüncüsü. Hava boşluklu kurulumun **tek** cevabı
  `market.offlineBundle` politikası oluyor, dolayısıyla o artık isteğe bağlı bir
  kurumsal ekstra değil, birinci sınıf bir kurulum yolu. Bir kez çekilmiş
  registry önbellekte kalır; yalnızca hiç çekmemiş bir makine engellenir. CI ve
  paketleme testleri ağa bağlanamaz, bu yüzden depoda pinlenmiş bir test
  registry'si zorunlu hâle geliyor.

### 0012 — Kapatmak veri silmez; silen fiil kaldırmaktır

- **Status:** accepted
- **Decision:** `service_disable`'ın bugünkü davranışı — container'ı silmek,
  image'ı silmek, adlandırılmış volume'leri silmek — `market_uninstall`'a
  taşınıyor. Üç fiil oluyor: `instance_disable` container'ı durdurup siler ve
  **veriye dokunmaz**; `instance_remove` örneği tablodan çıkarır ve veriyi
  sorar; `market_uninstall` paketi, image'ı ve — `purgeData` ile — veriyi
  siler. Gerekçe tek örnekli dünyada geçerliydi ve orada kalıyor: bir servis
  kapalıysa gerçekten kapalı olmalı. Ama bir *sürümü* geçici olarak kapatmak,
  o sürümün veritabanını silmek olamaz — mysql 8.0'ı 9.4'ü denemek için
  kapatan biri 8.0'ın verisini geri istiyor.
- **Consequences:** Davranış değişikliği ve sürüm notunda açıkça yazılması
  gerekiyor — bugünkü "kapat"ı temizlik olarak kullanan biri artık disk
  dolduracak. `discard_service`'in volume listesini şablondan okuyan mantığı
  korunuyor ama paket manifestinin `volumes[].purgeable` alanına dayanıyor,
  regex'e değil. Kapalı bir örneğin portu rezerve kalmaya devam ediyor.

### 0013 — Paketler statik HTTPS ile taşınır

- **Status:** accepted
- **Decision:** Dağıtım biçimi imzalı bir `registry.json` ve HTTPS üzerinden
  çekilen düz dosyalar. OCI artefaktı (ORAS) reddedilmedi, **ertelendi**:
  kurumsal ayna ve kimlik doğrulamayı Docker'dan devralma avantajları gerçek,
  ama yeni bir istemci bağımlılığı ve ikinci bir imza ekosistemi demek. Kaynak
  bir `PackageSource` trait'inin arkasında duruyor, böylece ikinci taşıma
  biçimi bir yeniden yazım değil bir uygulama olur.
- **Consequences:** Altyapı herhangi bir CDN, GitHub Pages dahil. Kurumsal ayna
  `market.registryUrl` ile bir dosya sunucusuna işaret ediyor, registry
  aynasına değil. `reqwest` zaten bağımlılık; yeni crate yok. Docker Hub
  oran sınırları paket indirmeyi etkilemiyor — yalnız image çekmeyi, ki o
  zaten bugünkü durum.

### 0014 — Depo desteklenen sürümleri taşır, `latest` bir dizin değildir

- **Status:** accepted
- **Decision:** Paket deposu 109 sürümün tamamıyla başlamıyor. Yayımlanan
  küme iki kümenin birleşimi: (a) upstream'de hâlâ bakım gören seriler,
  (b) bugün bir kullanıcının `.env`'inde yazılı olabilecek her sürüm — göç
  bunu gerektiriyor. Kalanlar `support.status: "eol"` ile işaretlenip
  yayımlanabilir ama listede öne çıkmaz. Ve `latest` bir sürüm dizini
  **olamaz**: sabitlenmiş bir digest'i, dolayısıyla bir hash zinciri yoktur.
  Registry düzeyinde bir takma ad oluyor — `recommended` alanı — ve göç
  `SERVICE_<ID>_VERSION=latest`'i o anki somut sürüme çözüp `instances.json`'a
  **somut olarak** yazıyor.
- **Consequences:** Bugünkü 25 varsayılanın **11'i** `latest`; göç bu 11'i
  somutlaştırmak zorunda ve bu, kullanıcının kurulumunu bugün olduğundan daha
  belirlenebilir yapıyor. "Desteklenen" bir görüş değil ölçüm olmalı:
  `tools/eol.mjs` her manifestin `support` alanını endoflife.date'e karşı
  doğruluyor ve sapma PR'ı kırıyor. Bir kez yayımlanmış sürüm registry'den
  **silinemez** — yalnız işaretlenebilir; silinirse o sürümü kurmuş bir
  `instances.json` ortada kalır.

### 0015 — Registry ayrı bir anahtarla imzalanır

- **Status:** accepted
- **Decision:** İçerik imzası, Tauri güncelleyicisinin binary imzasından ayrı
  bir ed25519 anahtar çifti kullanıyor. §5'in 4. maddesiyle aynı turda
  çözülüyor ama aynı anahtarla değil: biri binary'yi imzalar, diğeri
  kullanıcının makinesinde Docker'a verilecek tanımları. Saklama yeri, erişim
  ve rotasyon prosedürü **ortak**; anahtarlar ayrı.
- **Consequences:** İki anahtar, iki sızma yüzeyi ama tek bir sızmanın etkisi
  yarıya iniyor: güncelleyici anahtarı sızarsa sahte binary, içerik anahtarı
  sızarsa sahte paket — ikisi birden değil. Rotasyon baştan tasarlanmak
  zorunda: `known_keys.json` birden çok anahtar taşıyor ve yeni anahtar
  eskisiyle imzalanmış bir kayıtla tanıtılıyor. Rotasyon planı olmayan bir
  pinleme, sızma anında tek çözümü "herkes uygulamayı güncellesin" olan bir
  pinlemedir.

---

## 7. Ölçüm

Mekanik olarak sayılabilenler koda karşı tutuluyor:
`src-tauri/tests/platform_matrix_claims.rs` yanlış bir sayıda build'i kırıyor.

| | Sayı | Nasıl sayıldı |
|---|---|---|
| Toplam IPC komutu | **185** | `contracts/ipc.json` → `commands` (182 Rust + 3 `frontend-plugin`) |
| Bunlardan `#[tauri::command]` olarak yazılmış | **181** | `commands.rs`, `#[cfg(test)]` dışı |
| Frontend kaynak dosyası | **107** | `src/**/*.{js,vue}`, spec dosyaları hariç |
| Bunlardan `@tauri-apps` kullanan | **19** | aynı küme içinde metin taraması |
| **Veri katmanının geçtiği fonksiyon** | **1** (`src/lib/ipc.js` → `call()`) | `invoke(` `ipc.js` dışında **0** yerde geçiyor |
| `ipc.js` sarmalayıcısı | **178** | `api` nesnesinin üye sayısı |
| Rust kaynağı | **71 modül, 53.643 satır** | `src-tauri/src/*.rs` |

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
