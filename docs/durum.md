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

## 1. Teslim edilenlerin kaydı nerede

Bu bölüm bir kayıt tutuyordu ve dosyanın yarısına çıkmıştı — 990 satır, hepsi
bitmiş işin gerekçesi. Bu dosyanın işi **kalanı** göstermek, ve içinde ne kadar
çok biten iş birikirse "ne kaldı" sorusunu cevaplamak o kadar zorlaşıyor.

Bitenlerin kaydı dört yerde duruyor ve hiçbiri kaybolmadı:

* **`CHANGELOG.md`** — her teslimatın ne olduğu ve neden öyle yapıldığı,
  kullanıcıya bakan dille.
* **`docs/servis-market-mimarisi.md`** — paket ve market mimarisinin tamamı,
  fazlarıyla.
* **§6 (Kararlar)** — geri alınamayan tercihler, ADR olarak, gerekçeleriyle.
  Bir sonraki okuyucunun *neden öyle yapıldığını* aradığı yer burasıdır.
* **git geçmişi** — her satırın hangi turda ve neden değiştiği.

Bir madde §2'den ya da §3'ten çıktığında satırı silinir ve gerekçesi bu dört
yerden birine yazılır. §8'in kuralı budur.

---

## 2. Rekabet boşlukları — kalan

Sahadaki on ürüne karşı ölçüldü (Herd, Lerd, EnvKit, FlyEnv, ServBay, ForgeKit,
Laragon, Laradock, DDEV, XAMPP). **Mimari olarak en yakın rakip DDEV** — Docker
tabanlı, proje başına stack, paylaşılan Traefik router, mkcert HTTPS, repoya
işlenen config — ve en zayıf tarafı tam da StackVo'nun en güçlüsü: resmî GUI'si
terk edilmiş durumda.

### C — Genişletilebilirlik

**C-1 ve C-2 çıktı.** `authoring.rs` paketi yazıyor ve mühürlüyor,
`policy.market.allowedSources` kurumsal kapıyı tutuyor.

Kalan, bu turun işi değil ve mimari raporun kendi kararı: **üçüncü taraf
*dağıtımı* v1'de yok** (`servis-market-mimarisi.md` §4.6). Bir moderasyon
süreci, bir yayıncı kimliği ve bir kaldırma mekanizması gerektiriyor; mimarinin
hazır olması — kaynak alanı, imza doğrulayıcı, policy validator, ve artık
`allowedSources` — yeterli, kapının açılması ayrı bir karar. İmzalama ve yayın
yok; kendi makinesi ya da kurumunun kendi aynası için paket yazmak bunların
hiçbirini gerektirmiyor.

DDEV'in kayıt defteri (`addons.ddev.com`), 36 resmî ve 100+ topluluk eklentisi
var. Container tabanlı bir araç için kullanıcının kendi compose dosyasını
reddetmek, container tabanlı olmayı alternatifinden *daha kötü* yapan tek şey —
ve artık reddedilmiyor.

### E — Ağ

**E-1, E-2 ve E-4 çıktı.** E-2'nin joker yarısı E-1'in arkasındaydı ve sonek
eşleşmesinden bedavaya düştü — `a.b.shop.loc` de `shop.loc` de cevap alıyor.

E-1'in kalan yarısı "hangi dosyaya yazılacağı" sorusuydu ve cevabı tahmin etmek
değil **sormak** oldu: Linux'ta NetworkManager'ın dnsmasq'ı, makinenin kendi
dnsmasq'ı, systemd-resolved — bu sırayla aranıyor, hiçbiri bulunamazsa hiçbir şey
yazılmıyor ve satır gösteriliyor. Windows'ta mekanizma **var**: NRPT bir ad
uzayı ile bir sunucu alıyor, yalnız o soneke uygulanıyor. Önceki turun "Windows'ta
sonek başına mekanizma yok" cümlesi, platform hakkında değil, ne arandığı
hakkında bir cümleymiş.

Bunu, çalıştırmadan doğrulanamayacak bir dağıtıma yazmayı kabul edilebilir yapan
şey ölçüm: değişiklik uygulandıktan sonra **makinenin kendi çözümleyicisine**
soruluyor — sonek altındaki bir ad loopback dönmeli, ve değişiklikten önce çözülen
genel bir ad hâlâ çözülmeli. Biri düşerse değişiklik **geri alınıyor**. Yazılan
dosyayı geri okumak, yalnız yazmanın olduğunu kanıtlar.

Aynı sınıftan iki şey daha kapandı, ikisi de "yazdık ve unuttuk" hatası:
`/etc/resolver/test` dnsmasq'ın, Valet'in ya da bir meslektaşın betiğinin olabilir
— üzerine yazmak bir soneki başka bir araçtan geri dönüşsüz almak demek; artık
önce kenara kopyalanıyor, kapatınca geri konuyor. Ve sonek değişince eski dosya
kalıyordu: `.loc` artık **reddediliyordu**, yani yukarı gidip dürüstçe düşmek
yerine bu makinede kesiliyordu. İkisi de panelde yazıyor.

Doktor sayfasına da tek satır eklendi ve yalnız tek bir durumda görünüyor:
makine bizi soruyor, port başkasında, hiçbir ad çözülmüyor — uygulama,
konteynerler ve proxy sapasağlam görünürken. Bunu bildiren başka hiçbir şey
yoktu.

Yolda bulunan ve ilk turun kaçırdığı hata: her REFUSED ve her NODATA, gövdesinde
soru taşımadan başlığında "bir soru" diyordu. `dig` bunu okunan satırın bir üstünde
söylüyordu — *"Message parser reports malformed message packet"* — ve probe yalnız
`status:` satırına bakıyordu. Hoşgörülü bir araç yine de okur; bir stub çözümleyici
gönderdiği soruyla eşleşmeyeni atar, ve atılan bir cevap hızlı bir hata değil, beş
saniyelik bir zaman aşımıdır. NODATA yolu istisna da değil: her Chrome ve Safari
sayfa yüklemesi adresten önce bir HTTPS kaydı (tip 65) soruyor.

### F — Gözlemlenebilirlik: en büyük ürün boşluğu

**F-3 çıktı ve tabloyu daha iyi çizerek çıkmadı.** Flame graph yığınlardan
kurulur — her ölçüm kendi yolunu taşır, yani iki yerden çağrılan bir fonksiyon
kendi genişlikleriyle iki kutudur — cachegrind ise *kenar* tutuyor: "A, B'yi
çağırdı"nın her yerdeki toplamı. `profile::call_tree` bunu kendi yorumunda
söylüyordu ve ekran dürüstçe "çağrı ağacı" diyordu. Dosyada olmayan bilgi
düzenlemeyle geri gelmez; girdinin değişmesi gerekiyordu.

Xdebug'ın diğer türü zaten var: `xdebug.mode=trace` + `trace_format=1` her
fonksiyon **girişi ve çıkışı** için derinlikli, zaman damgalı bir satır yazıyor.
Aradaki boşlukları o anki yığına yazmak, her ayrı yol için "bu yol yığında ne
kadar durdu"yu veriyor — flame graph'ın genişliği tam olarak budur. Üçüncü bir
Xdebug kipi (`trace`), onay kutusu değil: farklı dosya, farklı ayrıştırıcı,
kaydetmesi çok daha pahalı.

Çalışan yığında ölçüldü (`examples/trace_probe.rs`): iki farklı üstten çağrılan
`slow()`, 60ms ve 10ms istenmişken **62.089µs ve 11.167µs olarak iki ayrı kutu**
döndü. Cachegrind'in söyleyemediği cümle bu.

**Yolda bulunan üç kırık — üçü de çalıştırarak, okuyarak değil:**

* **Profil alma hiç dosya yazmamış.** `xdebug.output_dir` ilk günden
  `/var/log/xdebug` diyor ve o dizini bağlamanın iki tarafında da kimse
  oluşturmuyordu. Xdebug var olmayan dizine sessizce yazmıyor: profil aç, tetikle,
  liste boş — hiçbir yerde hata yok. Artık her compose çağrısından önce
  oluşturuluyor.
* **MariaDB 12'de konuşacak istemci yokmuş.** MariaDB 11 `mysql*` sembolik
  bağlarını kaldırdı, 12 onlarsız geliyor; `mariadb:12` konteynerinde `mariadb`
  ve `mariadb-dump` var, `mysql` yok — uygulamanın her veritabanı özelliği ise
  ondan `mysql` istiyordu. Dökme, geri yükleme, anlık görüntü, taşıma ve sorgu
  günlüğü; hepsi, katalogdaki bir servis üzerinde. Birim testleri boyunca geçti,
  çünkü onlar argüman *listesini* denetliyor ve liste, adını verdiği program için
  doğruydu. Artık hangisinin olduğunu konteynerin kendisi seçiyor.
* **Mongo sorgu günlüğü taze bir veritabanında hiçbir şey kaydetmiyordu, kaydedince
  de okunmuyordu.** Profil Mongo'da veritabanı başına ve düğmeye basıldığı anda var
  olanlara uygulanıyordu — yeni başlamış bir konteynerde hiç yok, yani anahtar
  hiçbir şeyi açmıyor ve dürüstçe "kapalı" diyordu; üstelik günlük durum (uygulama
  ilk yazmada veritabanını oluşturur) tam da kaçırılan durumdu. Oturumu artık
  `admin` taşıyor ve her okuma, o sırada beliren veritabanlarını da açıyor.
  Kaydedileni ise ham gösteriyordu: satır başına beş yüz karakter `$clusterTime`,
  imza, oturum kimliği ve okuma tercihi, `find` ile `filter` ortada bir yerde.
  Sürücü zarfı artık hem gösterilenden hem şekilden çıkarılıyor — tek liste, ki
  birinde gürültü olan bir anahtar diğerinde kalmasın.

`examples/querylog_probe.rs` son üçünü bulan ve bulunmuş tutan şey: ayakta olan
her veritabanına karşı kaydı açıyor, geri geldiğinde tanıyabileceği bir soru
soruyor — F-1'in var olma sebebi olan N+1 şekli dahil — oturumu okuyor ve her
veritabanını bulduğu hâle geri bırakıyor.

**Ölçülmeyen tek yarım Postgres:** bu çalışma alanında kurulu değil. Probe onu
atlıyor ve atladığını yazıyor; kuruluysa aynı komut ölçer.

F-6 bu bölümün *uygulama* yarısıydı ve kapandı: bir konteynerin sağlıklı olup
olmadığı, neden durduğu (137 = bellek), kaç kez yeniden başladığı ve ne kadar
yediği artık ekranda.

### G — Veritabanları

**G-4 çıktı.** `dbmove.rs`: dökme ve geri yükleme zaten vardı, olmayan şey
"bu işe yarayacak mı" sorusunun cevabıydı. Aynı motor serbest, MySQL↔MariaDB
uyarıyla serbest, aile değiştirmek reddediliyor — bir `mysqldump` dosyası
Postgres girdisi değil ve `psql`'e verilmekle olmuyor; kabul etmek hedefi
boşaltıp binlerce sözdizimi hatasıyla düşmek olurdu. Sürüm düşürme uyarılıyor,
çünkü sessizce kırılan yön o ve kırıldığında hedef çoktan değiştirilmiş oluyor.

### H — Üretim köprüsü

**H-1 çıktı** (push + reçete). Zor yarısı zaten bitmişti: soyu geliştirme imajı
olan üretim imajı, çalıştırılıp sorularak temiz olduğu kanıtlanmış. Kolay yarı
bir kural etrafında kuruldu: **bir imaj yalnız doğrulandıktan sonra push
ediliyor.** Registry katmanları saklar; etiketi silmek içindekini kaldırmaz ve
paylaşılan bir registry'de birileri çoktan çekmiştir. Registry adı taşımayan
etiket de reddediliyor — `docker push myapp:v1` giriş yapılmış hesabın altında
Docker Hub'a gider.

Kimlik bilgileri bu uygulamanın işi değil: `docker login` kullanıcınındır,
`~/.ssh/config`'in `git.rs`'te olduğu gibi.

**"Sağlayıcıdan pull" bilerek dışarıda.** `release_load` bir tarball'ı zaten
getiriyor; bir registry'den çekmek `docker pull` ve onu bu uygulamanın içine
sarmak, kullanıcının zaten sahip olduğu bir komutun ikinci ve daha kötü bir
kopyası olurdu — reçete zaten imajın tam adını yazıyor.

### I — Performans: Docker eleştirilerinin doğru olanı

**I-1 çıktı.** Kalan iş "bir senkron katman" diye yazılıydı ve yazılan o olmadı —
çünkü ölçüm, kazancın nerede olduğunu söyleyince tasarım değişti.

`mount_bench` genel soruyu cevaplamıştı: `:cached`/`:delegated` **atıl**,
bind→volume mesafesi metadata ve yazmada **2–3 kat**. Yeni ölçüm
(`examples/perf_layer_bench.rs`, bu makinede, 8.000 dosyalık bir `vendor/` ile)
özelliğin cevaplaması gereken daha dar soruyu soruyor — "kaynağım editörün
gördüğü yerde kalırken ne kadar hızlanır":

| | bind | vendor birimde | + storage/framework |
|---|---|---|---|
| boot (framework açılışı) | 1,47s | 0,39s — **3,8×** | 0,40s |
| stat (ağaç yürüyüşü) | 0,42s | 0,39s | 0,34s |
| write (istek başına yazma) | 1,14s | 1,21s — **yok** | 0,41s — **2,8×** |

İki satır tasarımı belirledi: `vendor` açılışı alıyor ve yazmaya **hiçbir şey**
yapmıyor; yazmayı alan `storage/framework`. Yani "hızlandır" diye tek bir anahtar,
kazancın nereden geldiğini gizlerdi ve taşıdığı dizinler birinin projesi hakkında
bir tahmin olurdu. Bu yüzden özellik bir dizin listesi.

**Mutagen paketlenmedi ve uygulama içine çift yönlü senkron yazılmadı.** İkisinin
de gerekçesi `src-tauri/src/perf.rs` başlığında: biri üç platform için ikinci bir
ikili, diğeri yarım yapıldığında sesizce birinin dosyasını kaybeden bir sınıf
problem. Senkrona gerek de kalmıyor: bu dizinleri host'ta kimse yazmıyor.

Bedeli ekranda yazıyor — editör `vendor/`'ı artık göremez — ve `perf_export` onu
tek tıkla host'a geri kopyalıyor (anlık görüntü olduğu söylenerek). İki uçurum da
kapalı: taze bir birim **boş** başlar, o yüzden `perf_set` **önce** host kopyasını
içeri alıyor ve kopyalama düşerse ayarı hiç yazmıyor; birimi silmek ise anahtarın
yan etkisi değil, ayrı bir eylem.

Çalışan Docker'a karşı doğrulandı: compose ikinci dosyanın `volumes:` listesini
**ekliyor** (ezmiyor), ve birim bind'i alt yolda gölgeliyor — konteyner birimi
görüyor, host kopyası olduğu yerde kalıyor, konteynerin yazdığı host'a hiç
gitmiyor.

**I-2 çıktı** (`idle.rs`). Sinyal konteyner CPU'su değil — php-fpm hizmet
verirken de uyurken de sıfıra yakın, ağ sayaçları da sağlık kontrolü ve DNS için
kıpırdıyor; biri kullanılan şeyi durdurur, diğeri hiçbir şeyi durdurmaz.
Dürüst cevap proxy'nin: Traefik en son ne zaman hangi router'a yönlendirdiğini
zaten yazıyor. Generator erişim günlüğünü **iki alan tutup gerisini atarak**
açıyor (`RouterName`, `StartUTC`) — tek soruya bakan bir günlük, aynı zamanda
birinin kendi makinesinde gezdiği her URL'nin kaydı olmamalı.

Varsayılan **kapalı** ve günlüğün hiç anmadığı proje asla askıya alınmıyor:
biri bunu ilk kez açtığında her şeyin durması, özelliğin olabilecek en kötü
tanıtımı olurdu. **İstek üzerine uyandırma yok** ve nedeni yazılı: uyandırmak,
konteyner başlarken bağlantıyı açık tutabilen bir bileşen ister; istek yolundaki
tek şey Traefik ve o bunu yapamıyor.

### K — AI katmanı

**K-1 çıktı.** İki istemci eksikti ve ikisinin de gerekçesi `agents.rs`'in
başında yazılıydı; ikisi de o gerekçeyi ortadan kaldırarak kapandı.

**Codex** TOML kullanıyor ve birinin yorumlarını, anahtar sırasını, tırnak
stilini koruyarak TOML düzenlemek `toml_edit` istiyordu — bu depoda bir bağımlılık
ölçülen bir karardır. Ölçüldü: `toml_edit` ve `toml_writer` Tauri'nin kendi grafiği
üzerinden **zaten `Cargo.lock`'ta ve `NOTICE.md`'de**, yani kilit dosyası iki kenar
kazanıyor, sıfır paket. Şema da hatırlanmadı: bu makinedeki gerçek
`~/.codex/config.toml` `[mcp_servers.node_repl]` bloğunu `command`, `args`,
`startup_timeout_sec` ve iç içe bir `env` tablosuyla tutuyor, OpenAI'nin kendi
tanı belgesi de aynı bloğu belgeliyor.

**Zed** çalışan bir kopyaya karşı doğrulanamadığı için yoktu; hâlâ kurulu değil,
o yüzden şema Zed'in güncel yayımlanmış belgesinden alındı: düz
`"context_servers": { "<ad>": { "command": "…", "args": [], "env": {} } }`,
`source` anahtarı yok. Yolu ise belgede hiç yazmıyor ve Zed bazı şeyleri
`~/.config/zed`, bazılarını `~/Library/Application Support/Zed` altında tutuyor —
bu yüzden **ikisine de bakılıyor**; birini seçmek makinelerin yarısında sessizce
yanlış dosyayı yazmak olurdu.

**Ölçüm bir de eskiyi buldu.** `examples/agent_config_probe.rs` modülü bu
makinedeki **gerçek** dosyalara karşı çalıştırıyor: kopyaya kaydı yazıyor, geri
alıyor ve sonucu orijinalle **byte byte** karşılaştırıyor. Yeni TOML yolu ilk
denemede birebir geldi; JSON yolunun dördü gelmedi — çünkü `serde_json::Map`
`preserve_order` olmadan bir BTreeMap ve dosyayı **alfabetik sıraya** diziyordu,
üstelik girintiyi de kendi iki boşluğuna çeviriyordu. 58 KB'lık bir
`~/.claude.json`'ı tek bir kayıt eklemek için baştan sona değiştirmek, modülün
"dosyada olan her şey yerinde kalır" sözünün tam tersi. İkisi de kapandı: sıra
korunuyor (yine sıfır yeni paket — `indexmap` zaten oradaydı) ve dosya kendi
girintisiyle geri yazılıyor. Şimdi beş dosyanın beşi de birebir dönüyor; tek fark,
bilerek geride bırakılan boş `mcpServers` haritası.

**K-2 çıktı** (`agentctx.rs`). `agents.rs` host'taki asistanlara `stackvo-mcp`'yi
tanıtıyor; **konteynerin içinde** koşan bir agent için hiçbir şey yapmıyordu ve
yapamaz — `stackvo-mcp` host'ta stdio konuşuyor ve konteynerden ona giden bir
taşıyıcı yok. Böyle bir agent'ın ihtiyacı zaten bir sunucudan küçük: hangi
projede olduğu, sitenin adı, etrafında ne çalıştığı. Bu bir dosya.

`<proje>/.stackvo/context.json`'a yazılıyor, mount edilmiyor: `volumes:`'a bir
satır eklemek `fixtures_differential.rs`'i düşürürdü ve bu portun Bash
üretecini yeniden ürettiği kanıtı, daha derli bir mount'tan değerli. Ayrıca
agent'ın ilk baktığı yer zaten depo.

**Ad ve adres, asla kimlik bilgisi** — ve bu bir filtreyle değil, yapıyla:
`Service`'in id, host ve port'u var, sırrı koyacak alanı yok. Dosya birinin
kaynak ağacına yazılıyor ve kaynak ağacı yanlışlıkla commit'lenen bir şey.

**Hiçbir rakip MCP yüzeyini kontrol edilen bir kontrattan türetmiyor** — üç
kontrat testi her aracı `contracts/ipc.json`'a çapraz kontrol ediyor, var olmayan
bir komutu adlandıran araç build'i kırıyor. Bu gerçek bir farklılaştırıcı.

### L — Onboarding

**L çıktı.** Beş araç, üç ayrı şekil — ve satır zaten yarı yanlıştı: MAMP ile
Valet `imports.rs`'te yazılıydı ama **"kendim göstereyim" yolu onları
reddediyordu**. `imports_scan_at` beşten ikisini tanıyordu ve ekran da aynı ikisini
sunuyordu, yani MAMP'ı `/Applications` dışında olan birine "bu, uygulamanın
okuyabildiği bir araç değil" deniyordu. Taramanın hiç bulamayacağı iki araç
(Valet ve Sail) için ise o yol tek yoldu.

Üç şekil:

* **XAMPP, Laragon, MAMP** — tek bir site dizini. Laragon ayrıca site başına bir
  vhost yazıyor, adı oradan okunuyor.
* **Valet** — site dizini yok: dizin *park* ediyor (her çocuğu bir site) ve tek
  tek *link* ediyor (`Sites/` altında sembolik bağ). İkisi de, tld'si de kendi
  `config.json`'ından okunuyor.
* **Sail** — kurulum bile değil: her projenin *içinde* bir composer paketi.
  Tanıtıcısı `laravel/sail` adını geçen bir `docker-compose.yml`. Bu yüzden
  `well_known()` onun için hiçbir şey önermiyor — `~/Code` bir gelenek, makine
  hakkında bir olgu değil — ve gösterilen yol proje de olabilir, birkaç projeyi
  tutan klasör de.

Sail aynı zamanda **ne gerektiğini söyleyen tek kaynak**: compose dosyası mysql,
redis, meilisearch gibi servisleri sayıyor. Bunlar bu uygulamanın kataloğuna
eşleniyor (`pgsql`→`postgres`, `mongodb`→`mongo`), karşılığı olmayan sessizce
*atılıyor* — yerine benzeri konmuyor — ve içe aktarma neyi açması gerektiğini
söyleyebiliyor.

Ölçüldü (`examples/import_probe.rs`): beş aracın kendi dizin düzeni geçici bir
dizinde kuruluyor ve sevk edilen tarayıcı üzerinden geçiriliyor — XAMPP kendi
`dashboard`'ını atlıyor, Laragon'un vhost'undan `crm.test` çıkıyor, Valet'in park
edilmiş ve link edilmiş siteleri birlikte geliyor, Sail'in `pgsql`'i `postgres`
oluyor. Bu ölçüm bir hatayı da bulmuştu: Sail'in şablonu **dört boşluk** girintili
ve parser iki boşluğa sabitlenmişti — hiçbir servis bulunmuyordu.

### M — Küçük maddeler: on biri çıktı, biri iki parçaya ayrıldı

Önceki turda dördü teslim edilmiş, sekizi için "ucuz etiketi tutmuyordu"
denmişti. Doğruydu: ucuz değillerdi. Bu turda **maliyetleri ödendi** ve on ikisi
de tek tek ölçüldü.

| # | Madde | Durum | Nasıl |
| --- | --- | :-: | --- |
| M-1 | Proje favorileri | ✅ | `useFavourites` + yıldız sütunu. Tercih dosyasında, manifestoda değil: favori kişiye ait, `stackvo.json` ise commit ediliyor |
| M-2 | Mail gönderme / relay | ✅ | Mailpit'in **release** ucu + compose overlay. Yakalayıcı her şeyi yakalamaya devam ediyor; yalnızca elle iletilen mesaj çıkıyor |
| M-3 | Paylaşım URL'sinde QR | ✅ | Kendi kodlayıcısı. **macOS'un kendi çözücüsüne** okutuldu: yedi metnin yedisi bayt bayt aynı geri geldi |
| M-4 | Her siteyi listeleyen açılış sayfası | ✅ | Yığının **zaten sahiplendiği** ada bir sidecar. Canlı yığında ölçüldü: `https://stackvo.loc` yazılan sayfayı döndü |
| M-5 | Proje başına ortam değişkenleri | ✅ | `.stackvo/site.json` → compose overlay. Uygulamanın kendi `.env`'ine yazılmıyor; o dosya framework'ün |
| M-6 | Proje başına dizin listeleme | ✅ | nginx `autoindex`, Caddy `file_server browse`. Apache ve Swoole yapamıyor ve ekran bunu **söylüyor** |
| M-7 | Arayüz dilleri | ✅ | **Dil paketi**: config dizinine bırakılan bir JSON. Dil eklemek artık kod değişikliği değil |
| M-8 | Alternatif yüzeyler | ◐ | Tepsi artık **pencereyi açmadan** proje başlatıp durduruyor. TUI, §5'teki A-1 kararına bağlı; PWA'nın dayanacağı HTTP yüzeyi yok |
| M-9 | Framework komutları | ✅ | Symfony, Django, Rails ve Ruby. **B-4 kilidi değil**: her satır hâlâ derlemeye gömülü |
| M-10 | SSH agent'ının container'a iletilmesi | ✅ | Ölçüldü: konteynerden `ssh-add -l` ajana ulaşıyor |
| M-11 | Stripe webhook dinleyicisi | ✅ | Gerçek imajla ölçüldü — hesabın gerektirdiği yere kadar, ve o çizgi yazılı |
| M-12 | `.loc` için OAuth callback | ✅ | Tanımı okununca küçüldü: **yönlendirme tarayıcıya gidiyor**, sağlayıcı adresi çözmüyor |

**M-2 servis paketine dokunmuyor.** Mailpit ayarlarını kendi ortamından okuyor;
bu uygulama oraya `site.rs` ve `perf.rs`'in kullandığı **compose overlay** ile
uzanıyor — paket yeniden mühürlenmiyor, relay ayarlamamış bir çalışma alanı
öncekiyle aynı baytları üretiyor. Yakalayıcının compose anahtarı **imajından**
bulunuyor (`axllent/mailpit`): `.env` alanında `mailpit`, instance tablosuna
geçmiş olanda `mailpit-1-30`. İzinli alıcı listesi Mailpit'e bir düzenli ifade
olarak gidiyor, o yüzden noktalar kaçırılıyor — kaçırılmasa `me@test.com`,
`me@testxcom`'a da izin verirdi ve o birinin sahip olabileceği bir adres.

**M-3'ün asıl bulgusu ölçümün kendisi.** Birim testleri geçiyordu: alan
aritmetiği, yayımlanmış Reed-Solomon örneği, biçim ve sürüm dizeleri, çerçeve.
Sonra macOS **hiçbirini okuyamadı** — format bilgisinin ilk kopyası 8. sütun
yerine 8. satıra yazılmıştı, yani doğrunun devriği. Maskeyi o alan söylüyor,
dolayısıyla geri kalan her şeyin doğru olması hiçbir şeye yaramıyordu. Kendi
beklentisine karşı sınanan bir kodlayıcı, yazarıyla hemfikir olan bir kodlayıcı.

**M-4 yeni bir ad istemedi.** `core_domains` çıplak son eki zaten `/etc/hosts`'a
yazıyor, `certs::required_domains` zaten onun için sertifika üretiyor — ve o
adres Traefik'in 404'ünü döndürüyordu. Yani madde "yeni konteyner + DNS +
sertifika" değil, "yığının zaten sahiplendiği ada cevap veren bir konteyner"
çıktı. `nginx:alpine`, çünkü Alpine'ın busybox'ı `httpd` applet'i olmadan
derlenmiş — okunarak değil çalıştırılarak bulundu: `httpd: applet not found`.

**M-7'de çeviri yapılmadı ve bu bilerek.** Engel hiçbir zaman 2.000 dize
değildi; **yeniden derleme** idi. Dil kümesi üç yerde sabitti, dolayısıyla bu
uygulamayı gerçekten çevirebilecek kişi hiçbirine dokunamıyordu. Artık bir dil,
config dizinine bırakılan bir dosya. Eksik dizeler İngilizce görünüyor — vue-i18n
anahtar bazında geri düşüyor — ve ayarlar ekranı **ne kadarının çevrildiğini
yazıyor**. Makine çevirisi yok: geri düşen bir dize dürüst, uydurulmuş bir dize
birinin bulup inanmaması gereken bir cümle.

**M-11'in ölçümü kendi hatasını buldu.** İlk hâli tünelin sidecar'ını kopyalayıp
`--rm` kullanıyordu. Geçersiz anahtar CLI'ı çıkarıyor, `--rm` konteyneri
**log'uyla birlikte** siliyor, `status_all` hiçbir şey bulamıyor — bu özelliğin
en olası hatası için panel ne dinleyici ne hata ne de sebep gösteriyordu. Probe
de bunu bir süre başarı sandı, çünkü Docker'ın kendi "No such container" mesajı
"Error" ile başlıyor.

**M-8 ikiye ayrıldı ve yarısı zaten vardı.** `closeBehaviour: tray` ve
`startMinimized` yıllardır orada; eksik olan tepsinin **bir şey yapabilmesi**
idi — her satır pencereyi öne getiriyordu, yani tepsi bir yüzey değil kısayoldu.
Artık başlat/durdur pencereyi açmadan çalışıyor ve bildirim gönderiyor. Renkli
nokta üst seviyede kaldı: "yığın ayakta mı" bir bakış sorusu ve her satıra fiil
koymak cevabın enini iki katına çıkarırdı. TUI ikinci bir yüzey (§5, A-1); PWA
için dayanacak bir HTTP yüzeyi yok.

**M-9, B-4'ün kilidi değil.** B-4 *çalışma alanının* beyan ettiği komut; buradaki
her satır hâlâ derlemeye gömülü ve webview yalnızca bir id gönderiyor. Rails
`Gemfile` ile değil `bin/rails` ile bulunuyor — Gemfile en az Rails kadar sık
Sinatra ve Jekyll demek — ve `bundle exec` ile çalışıyor: binstub yalnızca
çalıştırma biti checkout'tan sağ çıktıysa çalışır, `bundle exec` ise bite değil
gem'e bakar.

### N — Sahada yalnız Lerd'de olan

| # | Madde | Durum |
| --- | --- | :-: |
| N | Worktree başına ortam | ⬜ |

`git worktree add` dala kendi subdomain'ini, kendi veritabanını, kendi
`.env`'ini veriyor. Container tabanlı bir araç için Podman tabanlı olandan
*daha* doğal — dal başına veritabanı bir volume adı, dal başına yönlendirme bir
Traefik kuralı. Kimsenin hızlıca kopyalayamayacağı tek özellik istenirse aday bu.

### Karar bekleyenler — bu turun işi değil

Aşağıdakiler kod eksikliğinden değil, verilmemiş bir karardan bekliyor. Kararlar
**§5'te**; biri verildiği gün ilgili satır buraya değil, yukarıdaki bölümüne
geri döner.

#### A — Yardımcı CLI ve host kabuğu (A-1, A-3)

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| A-1 | Yardımcı CLI | ⬜ | `src-tauri/src/bin/` yalnızca `stackvo-mcp.rs` |
| A-3 | Host kabuğu entegrasyonu (`stackvo php …`) | ⬜ | A-1'in arkasında |

**A-2 çıktı** (⌘K / Ctrl+K komut paleti). Kalan ikisi de aynı karara bağlı ve o
karar §5'te duruyor: bir CLI, sözleşmeyle senkron tutulacak üçüncü bir yüzey.

On rakibin sekizinde CLI var. Maliyeti göründüğünden düşük: `progress.rs`'in
`ProgressSink`'i ve `Sink::Null` sayesinde MCP yolu hiçbir Tauri tipi
adlandırmıyor, yani ayrıştırma yapılmış — eksik olan bir argüman ayrıştırıcısı
ve bir ilerleme yazıcısı. §5'teki karar isteniyor.

#### B-4 — Kullanıcı tanımlı komut

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| B-4 | Kullanıcı tanımlı komut | 🔒 | §5'teki karara bağlı. B-2 ve B-3 çıktı |

#### D-1 — Yerel AI servisleri

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| D-1 | Ollama, Qdrant, pgvector | 🔒 | §5'te **ertelendi** olarak kayıtlı, kapsam dışı değil |

#### F-5 — Kendine ait REPL yüzeyi

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| F-5 | Kendine ait REPL yüzeyi | 🔒 | PTY üzerinden `tinker` — dürüst %90, ama tezgâh değil. `quickcmd.rs` bir uygulama içi paneli **bilerek** reddetti: "zaten yapılandırdıkları REPL'in yanında ikinci ve daha kötü bir REPL". Bunu geri almak bir karar |

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
| 12 | E2E | 🟡 | **Webview yarısı indi**: Playwright, gerçek motorda, 10 test + dört sayfada axe; CI'da adım var. `tauri-driver` yarısı yok ve **bu makinede olamaz** — Tauri'nin kendi belgesi macOS'u desteklemediğini yazıyor |
| 21 | Sürüm kanalları, kademeli dağıtım, geri alma | ⛔ | `tauri.conf.json`'da `channel`/`rollout`/`paused` yok; #2'nin arkasında |
| 22 | Platform kapsamı (Linux aarch64, Win ARM64) | ⬜ | dört hedef |
| 24 | RTL | 🟡 | bağ test edilmiş; `vuetify.js`/`i18n` içinde `rtl` yapılandırması yok |
| 25 | Erişilebilirlik beyanı (VPAT / EN 301 549) | ⬜ | Ön koşul artık var: axe dört sayfada gerçek motorda koşuyor ve ciddi/kritik sıfır. Beyanın kendisi yazılmadı |
| 27 | `list_projects` cache | 🟡 | gizli pencerede yavaşlama kapandı; cache yok |
| 31 | Air-gapped kurulum | 🟡 | gidiş-dönüş tam ve arayüzde; paket yolu yok |
| 33 | Sözleşme kapısının harici bağımlılığı | 🟡 | checkout var ama **suite A hiç koşmuyor** — bu makinede de `NO_MANIFESTS` |
| 34 | Web sürümü / HTTP ikilisi | ⬜ | `src/bin/` yalnız `stackvo-mcp.rs` |
| 35 | Windows ve Linux dallarının çalıştırılması | ⬜ | CI üç OS'ta koşuyor; ayrıcalık yolları koşmadı |
| 36 | `EMBEDDED`'ın servis yarısı | ⬜ | ADR 0016'dan sonra **yalnız göç için** duruyor: `handover` `.env`'i okuyor ve `SERVICE_*_ENABLE/VERSION` varsayılanlarına ihtiyaç duyuyor. Desteklenen hiçbir çalışma alanı göç bekler durumda kalmayınca gitmeli — `config.rs`'te 185 anahtarın yaklaşık yarısı |

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

### Ağacın kendisi: hangi dizin ne için, ve hangisi terk edilmiş

Sorulan altı dizin, ölçülerek. Yöntem her satırda aynı: kim okuyor, ne zaman
okuyor, ve paketlenmiş uygulamada var mı.

| Dizin | Ne | Paketlenmiş uygulamada | Karar |
| --- | --- | :-: | --- |
| `skeleton/core/templates/services/` | 25 servis şablonu | **silindi** | ADR 0016 |
| `skeleton/core/compose/`, `servers/` | `base.yml` ve üç sunucu config'i | **gömülü** | Kalıyor — aşağıda |
| `tools/` | 8 Node betiği | **yok** | Geliştirme + CI. Kalıyor |
| `contracts/` | 9 JSON + 2 md | **kısmen gömülü** | Kalıyor — aşağıda |
| `dist/` | Vite çıktısı | — | Üretilen, `.gitignore`'da |

**`tools/` yalnız geliştirme ve CI için, ve uygulamaya hiç girmiyor.** Sekizinin
her biri bir kapı ya da bir ölçüm: `validate-contracts.mjs` (CI'ın `contracts`
işi), `check-coverage.mjs` + `coverage-floors.mjs` (kapsam tabanları),
`check-bundle.mjs` + `bundle-budget.mjs` (paket boyutu bütçesi),
`generate-notice.mjs` (lisans bildirimi, `--check` ile kapı),
`measure-env-usage.mjs`, `make-fixtures.sh`. Hiçbiri `src-tauri`'den
çağrılmıyor ve hiçbiri bundle'a girmiyor. Silinecek bir şey yok.

**`contracts/` ikiye ayrılıyor ve ikisi de gerekli.** Dördü `include_str!` ile
**binary'ye gömülü** ve çalışma anında okunuyor: `env.schema.json`,
`ipc.json`, `php-extensions.json`, `compose-policy.json`. Üçü yalnız kapılar
tarafından okunuyor — `package.schema.json`, `registry.schema.json`,
`surface.lock.json` (sonuncusu `tests/contract_version.rs`'in son yayınlanmış
IPC yüzeyi) — yani depoda kalır, uygulamaya girmez. `project.schema.json` ikisi
birden: `manifest.rs` ona göre yazılmış, `validate-contracts.mjs` onu okuyor.
Terk edilmiş dosya yok.

**`dist/` bir çıktı**, `.gitignore`'un 83. satırında, ve Tauri onu bundle'a
gömüyor. Elle dokunulacak bir şey değil.

#### `skeleton/core` neden gömülü kalıyor, ve `~/.stackvo` sorusunun cevabı

Şablon dizini gidince `skeleton/` beş dosyaya indi: `README.md`,
`core/compose/base.yml`, ve `core/servers/` altında nginx, caddy, frankenphp
config'leri — 20 KB. Soru yerinde: bunlar da mı gömülü, ve
`~/.stackvo` altında olsalar daha doğru olmaz mı?

**Gömülüler**, `include_dir!` ile, ve gerekçe `skeleton.rs`'in başında yazılı:
`bundle.resources` dosyaları çalıştırılabilirin yanına kopyalar ve
`resolve_resource()` ile bulunur, o da `tauri dev` altında paketlenmiş
uygulamadakinden **farklı** çözülür — yani ancak paketledikten sonra ortaya
çıkan bir hata sınıfı, ki bulunacak en kötü zaman odur.

**Ama zaten `~/.stackvo`'ya çıkarılabiliyorlar**, ve mekanizma bundan daha
iyisi: `overridable`/`materialize`/`revert`. Varsayılan gömülü, kullanıcı bir
dosyayı devralmak isterse tek çağrıyla diske yazılıyor, `read_template` diski
**önce** okuyor, ve `revert` dosyayı silerek gömülü olana dönüyor. Yani
"düzenlenebilir olsun" ile "paketlenmiş uygulama hiçbir checkout olmadan
çalışsın" aynı anda sağlanıyor.

Hepsini baştan diske yazmak ikisini birden kaybettirir: taze bir kurulum
kopyalanmış dosyalarla başlar, `prune_pristine`'in çözdüğü "kullanıcı bunu
düzenledi mi yoksa öylece duruyor mu" sorusu geri gelir, ve bir sürüm
yükseltmesi kullanıcının dokunmadığı dosyaları güncelleyemez hâle gelir —
`an_older_installs_untouched_copies_are_swept_and_edits_are_not` testi tam
olarak o eski davranışın kaydı.

---

## 4. Önerilen sıra

Karar gerektirmeyenler arasından, etki ÷ efor ile.

1. **I-1'in kalanı: senkron katman.** Ölçüm indi ve soruyu kapattı:
   bayrak değil, çünkü `:cached` ve `:delegated` atıl. Kalan iş, `bind` ile
   `volume` arasındaki **2–3 katı** kapatacak bir alt sistem — ve o mesafe artık
   bir tahmin değil, bu makinede iki kez ölçülmüş bir sayı. Sıranın başında
   duruyor çünkü hâlâ listenin en sonuç doğurucu maddesi; ama artık ilk adımı
   bir ölçüm değil bir tasarım.
2. **#12'nin kalanı: `tauri-driver`, Linux CI'da.** Webview yarısı indi ve
   #25'in beklediği ölçümü verdi. Kalan yarı gerçek ikilinin bir
   sürücü altında koşması, ve o **bu makinede yazılamaz** — Tauri macOS'ta
   desteklemiyor. Yani bu madde bir CI işi, bir masaüstü işi değil.

**G-3, E-3, J-1/J-2 ve D-1 bu listeden çıktı** — teslim edildi ve satırları
silindi; kayıtları `CHANGELOG.md`'de.

**Sıra bittiğinde geriye kalan iki şey kod değil.** Biri §5'in altı kararı;
öteki, §2'nin hâlâ dolu olan bölümleri — en büyüğü **F** (sorgu logu, tek istek
zaman çizelgesi, flame graph), ve o container içinde bir toplayıcı gerektirdiği
için ayrı bir tur.

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
6. **Göç etmeyi reddeden çalışma alanı (S-16).** ✅ **Cevaplandı — ADR 0016.**
   Zorunlu göç, bir kapının arkasında. Üç seçeneğin bedeli §4'ün sonunda
   yazılıydı; seçilen birincisi, `CatalogueGate`'in deseniyle: plan yazılmadan
   gösteriliyor, `.env` yedekleniyor, kapı atlanabiliyor ve öteki tarafta
   servissiz bir uygulama var. `.env` dalı ve 25 şablon silindi.

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

### 0016 — Göç bir kapıdır, banner değil

- **Status:** accepted
- **Context:** `render_generated`'ın servis yarısı iki kaynaktan üretiyordu:
`instances.json` yoksa `.env` ve binary'ye gömülü şablonlar, varsa tablo ve
paket ağacı. İki dal, ikincisi yazılırken var olan her kurulum çalışmaya devam
etsin diye vardı. §5'in altıncı maddesi soruyordu: gömülü şablonlar silinince
göçü **reddeden** kullanıcıya ne olacak — zorunlu göç, bir sürüm boyunca iki
yol, yoksa açılışta sessiz göç.

İki yol zaten olan şeydi, ve bedeli D-1 ile somutlaştı: iki dal **farklı
kataloglar** biliyordu. `.env` binary'de şablonu olan 25 servisi, tablo ise
paket ağacında ne varsa onu. Solr ve ClickHouse paket olarak gelince
`services: ["solr"]` yazan bir proje doğru bir beyana yanlış bir uyarı almaya
başladı — ve uyarı düzeltilemiyordu, çünkü düzeltmek şablonsuz bir girdiyi
`.env` kataloğuna sokmak, yani "açık görünen ve var olmayan" bir servis
üretmek olurdu.

Sessiz göç, bir kullanıcının servis tanımlarını sormadan değiştirir. Bu kod
tabanı bundan küçük şeyler için bile izin soruyor: `env_reveal` bir parolayı
**okumayı** bir eylem sayıyor.

- **Decision:** Zorunlu göç, bir kapının arkasında. `.env` dalı silindi.
`MigrationGate`, `RequirementsGate`/`CatalogueGate`/`BootstrapGate` deseninin
dördüncüsü — katalogtan sonra (göç her servisi bir **pakete** çözüyor) ve
bootstrap'tan önce (bootstrap yığını üretiyor, bu neyden üretileceğine karar
veriyor). Plan yazılmadan önce gösteriliyor, `.env` önce
`.env.pre-market.bak`'a kopyalanıyor, ve Market sayfası geri alma panelini
koruyor.

Kapı **atlanabilir**, ve öteki tarafta servissiz bir uygulama var — ki bu
`CatalogueGate`'in katalogsuz makine için kurduğu argümanın aynısı: servissiz
StackVo hâlâ bir ters vekil, bir sertifika otoritesi ve bir proje koşturucusu.
Atlamanın **yapmadığı** şey eski yığını geri getirmek, çünkü onu kuran kod
artık yok.

- **Consequences:** Göç etmemiş bir çalışma alanı yığın kuramaz;
`render_generated` adıyla reddediyor (`Conflict` + `MIGRATE_THE_WORKSPACE`)
çünkü oraya kapı atlanmadan ulaşılamaz ve sessiz boş bir render en kötü cevap
olurdu. `skeleton/core/templates/services/` silindi (25 dizin, 128 KB);
`template::DYNAMIC_SERVICES`, `render_dynamic_compose`, `volume_names`,
`harvest_volumes`, `service_body`, `skeleton::all_service_templates`,
`shipped_services`, `collect_tpl_paths` ve `commands::env_service_files` ile
birlikte. `EMBEDDED`'ın servis yarısı **kalıyor** — göç onu okuyor — ve §3'e
36. madde olarak yazıldı.

Kataloğun iki listesi tekleşti: `env.schema.json`'ın `services`'i artık
şablonlarla eş tutulmuyor, bir **kelime dağarcığı** olarak okunuyor, ve solr
ile clickhouse oraya girdi. D-1'in bulduğu yanlış uyarı kapandı.

`handover_equivalence.rs`'in eşdeğerlik kanıtı korundu ve bedeli yazıldı: o
test göçün her imajı, portu ve volume'ü koruduğunu **iki tarafı da render
ederek** kanıtlıyordu; bir taraf gidince çıktısı donduruldu
(`tests/fixtures/golden/handover-before.yml`). Dondurulmuş taraf kayamaz —
dürüst sınır bu, ve `ENV` ile fixture artık bir çift.

---

## 7. Ölçüm

Mekanik olarak sayılabilenler koda karşı tutuluyor:
`src-tauri/tests/platform_matrix_claims.rs` yanlış bir sayıda build'i kırıyor.

| | Sayı | Nasıl sayıldı |
|---|---|---|
| Toplam IPC komutu | **238** | `contracts/ipc.json` → `commands` (235 Rust + 3 `frontend-plugin`) |
| Bunlardan `#[tauri::command]` olarak yazılmış | **234** | `commands.rs`, `#[cfg(test)]` dışı |
| Frontend kaynak dosyası | **128** | `src/**/*.{js,vue}`, spec dosyaları hariç |
| Bunlardan `@tauri-apps` kullanan | **20** | aynı küme içinde metin taraması |
| **Veri katmanının geçtiği fonksiyon** | **1** (`src/lib/ipc.js` → `call()`) | `invoke(` `ipc.js` dışında **0** yerde geçiyor |
| `ipc.js` sarmalayıcısı | **231** | `api` nesnesinin üye sayısı |
| Rust kaynağı | **89 modül, 71.847 satır** | `src-tauri/src/*.rs` |

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
3. **Bir madde bittiğinde satırı buradan silinir** ve kaydı `CHANGELOG.md`'ye,
   geri alınamaz bir tercih taşıyorsa §6'ya yazılır — kararı ve yolda bulunan
   hatayı içeren cümleyle. Bu dosyanın işi kalanı göstermek; içinde biriken
   bitmiş iş, "ne kaldı" sorusunun cevabını zorlaştırmaktan başka bir şey
   yapmıyor. Bir sonraki okuyucunun ihtiyaç duyduğu şey ne yapıldığı değil,
   neden öyle yapıldığı — ve o cümle §6'da ya da CHANGELOG'da duruyor.
