# StackVo — kalan işler, kararlar ve ölçüm

**Son ölçüm: 16 Ağustos 2026.** Bu dosyanın işi **kalanı** göstermek. Biten iş
buradan silinir; kaydı `CHANGELOG.md`'ye, geri alınamaz bir tercih taşıyorsa
§6'ya gider (§8).

`✅` bitti · `🟡` yarım · `⬜` başlanmadı · `⛔` engelli (dışarıdan bir şey
gerekiyor) · `🔒` karar bekliyor

**§2–§4'ün arkasında kapı yok ve olamaz** — "yapılmadı" kodun ölçülebilir bir
özelliği değil. Elde olan tek şey her satırın **nasıl bakıldığını** taşıması.
§5, §6 ve §7'nin arkasında **var**: karar tablosu ve ölçüm testlerle tutuluyor,
yanlış bir sayı build'i kırıyor.

---

## 1. Bitenlerin kaydı nerede

* **`CHANGELOG.md`** — her teslimatın ne olduğu ve neden öyle yapıldığı.
* **`docs/servis-market-mimarisi.md`** — paket ve market mimarisi; tarif ettiği
  iş bitince silinecek.
* **§6** — geri alınamayan tercihler, gerekçeleriyle. Koddaki "ADR 0005",
  "ADR 0009" atıfları bu tabloyu kastediyor; numaralar korundu.
* **git geçmişi** — her satırın hangi turda ve neden değiştiği.

---

## 2. Ürün boşlukları — kalan

Sahadaki on ürüne karşı ölçüldü (Herd, Lerd, EnvKit, FlyEnv, ServBay, ForgeKit,
Laragon, Laradock, DDEV, XAMPP).

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| C | Üçüncü taraf paket **dağıtımı** | ⛔ | **Kod tarafı kapandı** (ADR 0021): imza doğrulayıcı `signing.rs`'te ve `refresh` onu indeksi ayrıştırmadan **önce** koşuyor, anahtar rotasyonu (`known-keys.json`) ve emeklilik var, geri çekilmiş sürüm kuruluma **reddediliyor**, kurulu olanı `doctor` bildiriyor. Kalan üç şeyin üçü de kod değil: resmî anahtarın töreni (§5.3'ün arkasında), moderasyon süreci ve yayıncı kimliği kaydı. **Kurumsal ayna bugün çalışıyor** — kendi anahtarını `policy.market.additionalKeys` ile pinler |
| D-1 | Yerel AI servisleri (Ollama, Qdrant, pgvector) | ✅ | **Cevaplandı (ADR 0027): yalnız pgvector, ve bir servis değil — `postgres`'in bir sürümü.** Bu depoda değişen kod **hiç**, ve bulgu bu: ADR 0011 uygulamanın hiçbir servis tanımı taşımamasını kararlaştırdığı için yeni servis bir *paket*, ve uygulamanın zaten ifade edebildiği bir paket burada sıfıra mal oluyor. Ayıran şey bir capability: `vector`. `vector_capability.rs` cümleyi tekrar etmiyor, kanıtlıyor. Ollama/Qdrant isteyen `sidecars` yazar (ADR 0023) |

### Girilmeyecek kavgalar

Yeniden tartışılmaması için yazılı:

* **Native-binary hız savaşı.** FlyEnv "<100 ms açılış", Laragon "~10 MB RAM"
  yayınlıyor; kazanılamaz. Ama *soğuk açılış* ile *dosya G/Ç* ayrı sorular —
  birincisi ikincisini görmezden gelmenin bahanesi olmasın.
* **Çift yönlü senkron ve Mutagen paketleme** (I-1'in reddedilen yarısı).
  Gerekçe `src-tauri/src/perf.rs` başlığında: biri üç platform için ikinci bir
  ikili, diğeri yarım yapıldığında sessizce birinin dosyasını kaybeden bir
  sınıf problem. Gerek de yok — birime taşınan dizinleri host'ta kimse yazmıyor.
* **Sağlayıcı registry'sinden pull.** `docker pull`'un ikinci ve daha kötü bir
  kopyası olurdu; reçete imajın tam adını zaten yazıyor.
* **LLM sağlayıcı proxy'si** (ServBay'in AI Gateway'i). Kapsam dışı. Yerel AI
  *servisleri* farklı bir soru — §5.2.
* **FlyEnv'in 50+ aracı** (base64, QR, regex test ediciler). Odaksız.
* **Portable mod.** Docker bağımlılığıyla anlamsız.
* **Laradock'un 130 servisinin peşine düşmek.** Genişliğin kendisi için
  genişlik, bir kataloğun bakımsız hâle gelme yolu.
* **Ücretli katman.** Herd $99/yıl, ServBay $59/yıl, Laragon ticarileşip
  fork'landı. EnvKit, ForgeKit ve DDEV tam oradan saldırıyor; MIT o çizginin
  doğru tarafı.

---

## 3. Mühendislik borcu — kalan

Ürünün ne yapamadığı değil, **mühendisliğin** ne taşıyamadığı. Eksikler kod
kalitesinde değil, **kalitenin kod dışına, otomatik ve devredilebilir hâle
çıkarılmasında**: bugün 1 yazar var; ikinci geliştirici geldiği gün ya da
altıncı ayda hafıza soluklaştığında çalışmayacak olan şey bu.

| # | Madde | Durum | Nasıl bakıldı |
| --- | --- | :-: | --- |
| 2 | Güncelleme endpoint'i | 🟡 | **Karar verildi (ADR 0025) ve endpoint düzeltildi.** Eskisi iki bağımsız yönden yanlıştı ve ikisi de sessizdi: sahibi (`stackvo/…`, oysa remote `fahrettinaksoy/…`) ve mekanizması (`raw.githubusercontent.com/.../main/latest.json` — o dosyayı `main`'e yazan hiçbir şey yok; `tauri-action` onu release'in **içine** yazıyor). `dialog: false` olduğu için 404 hiç görünmüyordu. Artık `releases/latest/download/latest.json`, ve `updater_endpoint.rs` adresi `.git/config`'ten türetip karşılaştırıyor, `includeUpdaterJson` ile imzalama secret'ının workflow'da kaldığını da tutuyor. Kalan **kod değil**: anahtarın repository secret'ı olarak eklenmesi ve bir sürümün yayınlanması |
| 10 | Ön yüzün tipleri | ✅ | **Her iki yarı da bitti, ve ikincisi birincinin neden yetmediğini gösterdi.** `tauri-specta` bu depo için yanlış alet — burada **hiç TypeScript yok** ve çıktısı hiçbir şeyin okumadığı bir `.ts` modülü olurdu; bedeli üç crate ve 245 fonksiyona bir öznitelik. Tipler `contracts/ipc.json`'dan üretiliyor → `src/lib/ipc.d.ts`: **127 adlandırılmış tip, 242 sarmalayıcı, okunamayan 0 alan** (eskiden 24). **(a)** Sözleşmenin adlandırıp tanımlamadığı 19 tipin hepsi beyan edildi — `CpuStats`, `ContainerDetails`, `Timeline`, `Manifest`, … — artı onları taşıyan alt şekiller; `Plan` hiç yeni bir tip değildi, `PresetPlan`'ın kendisiydi ve `project_requirements_apply` artık onu adlandırıyor. **(b)** `npm run types:tsc` (`tsconfig.json`, `checkJs`, `noEmit` — derleme adımı yok) CI'da koşuyor. Kapıyı kurmak **tam da eksik olan şeyi buldu**: üretilen dosya *hiç derlenmemişti*, ve derlenmiyordu — birinin düzyazısındaki `docs/*/…` yolu JSDoc yorumunu erkenden kapatıyor, dosya o satırda ayrıştırılamıyor ve sonrası kod olarak okunuyordu; `OperationId` 24 kez adlandırılıp bir kez bile tanımlanmıyordu; `StackvoError`'ın kurucusu `Error`'dan miras alınıyordu, oysa uygulama onu bir **nesneyle** kuruyor. Üçü de üretecin kendi hatasıydı ve üçü de tek bir `tsc` koşusunun sorusuydu. Kalan: yok |
| 12 | E2E | 🟢 | **Koştu, ve koşmak dört ayrı kusuru buldu — üçü süitin kendisinde.** Playwright yarısı zaten vardı. `tauri-driver` yarısı `tools/linux/` ile bir konteynerde koşuyor. **(1)** İlk koşuda dört test düştü ve süit yeşil raporladı: `whyNotHere` Linux'ta `null` döndürüyordu ve `node:test` `null` bir `skip`'i direktif okuyor — düşen test `# skipped` altına yazılıp `# fail`'in dışında kalıyor ve süreç 0 ile çıkıyor. **(2)** Dört düşüşün mesajı da aynıydı ve hiçbir şey açıklamıyordu; düşen bir iddia artık sayfanın ve sürücünün ne dediğini basıyor, ve ilk baskıda cevap çıktı: `url: about:blank`, `"Could not connect to localhost"`. **(3)** Sebep `cargo build`'di — `tauri-build` düz bir cargo derlemesi için `cfg(dev)` yayıyor, yani ikili `devUrl`'i gömüyor ve webview `localhost:1420`'yi açıyor. Süitin manşet testi ("derlenmiş paket gerçek webview'de render oluyor") **süitin kendi seçtiği profille geçemezdi**. Eski yorum debug'ı maliyetle savunuyordu; maliyet gerçekti, karşılaştırma değildi — ikisi aynı soruyu farklı hızda cevaplamıyordu, biri cevaplamıyordu. Artık `npx tauri build --debug --no-bundle`. **(4)** `tauri-driver` bir vekil: dinlediği port o ayağa kalkar kalkmaz cevap veriyor, arkasındaki `WebKitWebDriver` ayrı başlıyor — erken sorulan oturum "connection refused" alıyordu; artık sınırlı bir yeniden deneme var. Beşincisi zarfı okumayan bir testti (`box.internals`, oysa `box.value.internals`) — geçmesi mümkün değildi ve dördünün mesajı aynı olduğu için tipo onların kılığındaydı. Sonuç: **5/5 geçiyor, 0 atlandı**. Kalanı CI'da ilk koşu |
| 21 | Sürüm kanalları, kademeli dağıtım, geri alma | ✅ | **Yazıldı ve bağlandı.** `tauri-plugin-updater` bir manifesto çekip sürüm karşılaştırıyor, imza doğrulayıp kuruyor — hepsi bu; kanal kavramı yok, yüzde yok, ve en önemlisi **durdurma** yok: bozuk çıkan bir sürüm geri çağrılamıyor. `channel.rs` o kararı veriyor, sıra tasarımın kendisi (`paused` her şeyi yener; `supersededBy` sürüm karşılaştırmasından önce gelir çünkü varlık sebebi onu yenmektir; dalga en sonda). Kova `sha256(installId:version) % 100` — rastgele olsaydı her kontrolde yeniden atılır, güncelleme belirip kaybolurdu. **Karar artık kullanıcıdan önce veriliyor**: `checkForUpdate` eklentinin *zaten* çektiği manifestoyu (`rawJson`) `updater_offer`'a veriyor, ikinci istek yok, iki cevabın ayrışması yok — ve reddedilen bir sürüm ekrana hiç çıkmıyor, çünkü sunup sonra kurmayı reddetmek hiç sunmamaktan kötü. Karar okunamazsa sürüm **sunuluyor**, reddedilmiyor: alanlar eklemeli ve bugünkü `latest.json` hiçbirini taşımıyor. 16 Rust + 12 JS testi. `getrandom` eklendi, kilit **tek satır** büyüdü |
| 22 | Platform kapsamı (Linux aarch64, Win ARM64) | 🟡 | `release.yml` **altı** hedef sayıyor, iki ARM satırı yerel ARM koşucularıyla. `rehearsal` girdisi bu satırları imzalama kararından ayırdı: altısını da derliyor, testi her birinde koşuyor, paketleri run sayfasına bırakıyor ve **hiçbir şey yayımlamıyor**. GitHub Actions burada koşturulamaz ve `release_rehearsal.rs` bunun tersini iddia etmiyor — koşucusuz da ayakta kalan iddiayı tutuyor: **yayınlayabilecek her adım kapılı.** `tagName` ifadesinin yönü de dahil; `inputs.rehearsal && '' || github.ref_name` daha derli toplu görünür ve `''` yanlış-değerli olduğu için `||` ateşler, prova dal adıyla yayın yapar — girdinin önlemek için eklendiği hatanın ta kendisi. Değiştirip düştüğünü doğruladım. Kalan: **birinin çalıştırması** |
| 31 | Air-gapped kurulum | 🟡 | **Paket yolu yazıldı.** Okuyan yarı `LocalSource`'tan beri vardı; **yazan** yarı yoktu — bir paketi üretmenin tek yolu paket deposunu klonlayıp düzeninin istemcinin okuduğu düzen olmasını ummaktı, ki bu bir kurulum yolu değil işe yarayan bir tahmin. `market::bundle` indeksi ve her paketi tek dizine yazıyor; çıktı **bir kaynak**: uzak uç `market_refresh` + `market_install`'ı ondan koşuyor ve bir checkout'tan ayırt edemiyor (test bunu iki ayrı çalışma alanıyla uçtan uca koşturuyor). `registry.json` **bayt bayt** kopyalanıyor — imza baytların üstünde (ADR 0015) ve `manifestSha256` onlardan zincirleniyor. Her manifest **burada**, ağı olan makinede doğrulanıyor; geri çekilmiş sürüm satırını koruyup dosyalarını bırakıyor (ADR 0014). Yüzey: `stackvo market-bundle <dizin>` — bir düğme değil, çünkü bunu yapan kişi ssh'tan koşan bir operatör. §9'un `stackvo-packages.tar`'ı bu dizinin **paketlenmesi**, ikinci bir mekanizma değil. **GUI karşılığı da var**: Ayarlar → Katalog panelinde bir klasör seçici; paket yazılınca boyutu MiB olarak, imzasızsa uyarıyı ve taşınmayan sürümleri yayıncının kendi sözleriyle gösteriyor — ikisi de koridoru yürümeden önce okunması gereken şeyler. Kalan: tar'ı üreten adım elle (`tar -cf … -C <dizin> .`) |
| 33 | Sözleşme kapısının harici bağımlılığı | ✅ | **Kusur bir yapılandırma değil bir tasarım sorunuydu.** Suite A `<root>/projects/` altındaki `stackvo.json`'ları okuyor; ne CI'nın checkout'unda ne de bir geliştiricinin makinesinde öyle bir şey var — yani manifest yarısı **hiçbir makinede, bir kez bile** koşmamıştı. Doğrulayıcının kendi girdisi yoktu, dolayısıyla koşup koşmaması yanında ne bulunduğuna bağlıydı. `tools/fixtures/validator-workspace/` o girdi: dört proje, **üçü kasten bozuk** (geçersiz sürüm, matriste olmayan eklenti, Bash çıkarıcısının sessizce düşüreceği bir ad), ve `tests/validate-contracts.spec.js` ürettiği kodları **tam olarak** doğruluyor. **Koşturmak iki ölü kontrol buldu**, ikisi de doğrulayıcının içinde: `EMBEDDED` kazıyıcısı düz bir dizi bekleyen bir regex'ti ve #36'nın bölmesiyle o gün eşleşmeyi bırakmıştı — **boş küme** döndürüyor, 20 anahtar yeniden "`.env`'de yok" diye raporlanıyordu; onarıldığında da yalnız **anahtar adlarını** okuyordu, oysa `.env` yalnızca bir ayar *değiştiğinde* yazılıyor, yani dokunulmamış bir çalışma alanında her proje "listede olmayan" bir PHP koşuyor görünüyordu. Uyarılar 21 → 1 |
| 34 | Web sürümü / HTTP ikilisi | ✅ | **Karar (ADR 0026), politika, taşıma ve arayüz.** Loopback, token, salt-okunur, artı keystore'a ulaşan sorguların reddi. **İkinci bir dispatcher yazılmadı**: `mcp.rs`'in araç tablosu zaten her aracı bir sözleşme komutuna bağlıyor, ve bir araç tele ancak **iki politika da izin verirse** çıkıyor — `!writes` ve `exposable(command)` — böylece keystore kuralı, keystore'dan hiç bahsetmeyen bir tabloya ulaşıyor (7 araç; `stackvo_doctor` dahil değil, çünkü `doctor` birkaç çağrı ötede keystore'a ulaşıyor). Soket ~40 satır `tokio::net`; `axum` tek metot, tek yol, tek başlık için fazla. **Varsayılan kapalı** ve Ayarlar'da bir panelden açılıyor: haberi olunmayan bir dinleyici, kapatılmayan dinleyicidir. Token **bir kez** dönüyor — `websurface_status` onu taşımıyor ve taşıyamaz, çünkü taşısaydı sonraki her çağırana verirdi ve bunların ilki yüzeyin kendisi. Soket testleri iki gerçek açık buldu: hiçbir şey göndermeyen istemci görevi süresiz tutuyordu (slowloris → 5 sn okuma süresi), ve `stop` gerçekten portu bırakıyor mu (bırakıyor, test bağlanmayı deneyerek doğruluyor). 24 + 9 + 8 test |
| 35 | Windows ve Linux dallarının çalıştırılması | 🟡 | **Linux yarısı artık koştu — ve koşturmak, koşmadığı için görünmeyen bir şeyi buldu: bu depo Linux'ta hiç derlenmiyordu.** `certs.rs`'in `#[cfg(not(target_os = "macos"))] ca_trusted`'ı `ca_common_name`'i çağırıyor ve o fonksiyon **hiçbir yerde tanımlı değildi**; macOS'ta dal derlenmediği için `cargo check` sessizdi. Artı üç `-D warnings` hatası (`menu.rs`'te yalnız macOS blokunda kullanılan `Submenu` ve `product`, `agents.rs`'te yalnız iki platformda gereken bir `mut`). Yani CI'nın Linux `build` job'ı da kırmızıydı. Onarıldı; `ca_common_name` bilerek `cfg` dışında — onu görünmez kılan şey tam olarak bir cfg kapısıydı. **`tests/elevate_probe.rs` Linux'ta geçiyor**: sahte `pkexec` `PATH`'te, gerçek `apply → elevate::run` zinciri, 126 iptal olarak okunuyor. Bunu mümkün kılan `tools/linux/` — CI'nın ortamını bir konteynerde yeniden kuruyor (aynı taban, aynı paketler, `rust-toolchain.toml`'dan pinlenmiş sürüm). Kalan: **Windows dalı**, ki o hâlâ burada derlenemiyor (`aws-lc-sys` Windows SDK'sı) |
| 36 | `EMBEDDED`'ın servis yarısı | 🟡 | ADR 0016'dan sonra **yalnız göç için** duruyor. `config.rs` `SETTINGS` (36, kalan) ve `LEGACY_SERVICES` (150, gidecek) taşıyor — "yaklaşık yarısı" yanlıştı, **beşte dördü**. Okuyan dört modül `legacy_env_claims.rs` ile kilitli. **Tarih artık verildi: 0.4.0** (§5). Ve düzyazı değil bir kapı: uygulama 0.4.0'a çıktığı ve sabit hâlâ orada olduğu an build kırılıyor; tarihin 1.0.0'a ötelenmesi de kırıyor. Kalan: **o gün silmek** |
| 37 | Testin gerçek keystore'a dokunması | ✅ | **Tam Rust koşusu askıda kalıyordu ve sebebi bir testti.** `env_writer`'ın `a_moved_key_is_taken_out_of_the_file_patch`'i `redirect_moved_keys` üzerinden **gerçek macOS Keychain'ine** yazıyor; macOS, soran ikili değiştiğinde izin soruyor — ki `cargo build` sonrası her seferinde değişiyor — ve cevaplayan olmayınca test süresiz bekliyor, bütün koşuyu da beraberinde götürüyor. Testin kendi yorumu "geliştirici makinesinde başarılı olur" diyordu; olmuyordu. Var olduğundan beri böyleydi ve görünmemesinin sebebi belirli: **asılan bir süit, yavaş bir süite benzer.** Çözüm bir env değişkeni **değil** — `hosts.rs`'in `STACKVO_HOSTS_PATH`'i ve `elevate.rs`'in `STACKVO_POWERSHELL`'i doğru şekilde, ama burada aynı desen *parolaları* yayınlanmış bir ikilide OS keystore'undan başka bir yere taşıyan bir değişken olurdu. `cfg(test)` kullanıldı: derleyici zorluyor, yayınlanan yapının ulaşabileceği ikinci bir arka uç yok ve bir unit test'in gerçek olana yolu yok. `cfg_regions.rs` üç işlemin **her birini ayrı ayrı** kontrol ediyor — ilk sürümü dosyayı bir dizgi torbası gibi okuyordu ve `write` kapısını kaybetmişken geçiyordu, çünkü `read` hâlâ taşıyordu |

---

## 4. Önerilen sıra

Karar gerektirmeyenler arasından, etki ÷ efor ile.

**Liste boş.** Yazılmamış kod da, bağlanmamış uç da kalmadı. Kalan her maddede
kalan şey bir **koşu**, elle bir **adım**, bir **tarih** ya da bir **süreç**:

* **#12** — driver süiti bu makinede konteynerde **5/5, 0 atlandı**. Kalanı
  CI'da ilk koşu.
* **#22** — GitHub Actions burada tetiklenemiyor. Koşucusuz ayakta kalan iddia
  `release_rehearsal.rs`'te; kalanı birinin çalıştırması.
* **#35** — Linux yarısı geçiyor. Kalanı **Windows dalı**, ki burada
  derlenemiyor bile (`aws-lc-sys` Windows SDK'sı).
* **#31** — tar'ın elle üretilmesi (`tar -cf … -C <dizin> .`).
* **#2** — anahtarın secret olarak eklenmesi ve bir sürümün yayınlanması. Bu tek
  adım #22'nin provasını gerçek bir yayına, #21'i sahada sınanabilir bir şeye
  çeviriyor ve §2 C'nin son iki maddesinin önünü açıyor.
* **#36** — 0.4.0'da silmek; kapı o gün build'i kırıyor.
* **§2 C** — moderasyon süreci ve yayıncı kimliği kaydı; kod değil.

Bu turların bulgusu hep aynı yerden geldi: **koşmayan bir şey doğru görünür** —
ve bir varyantı: **asılan bir süit, yavaş bir süite benzer.**
Suite A hiç manifest okumamıştı, driver süiti hatayı başarı sayıyordu, bu depo
Linux'ta hiç derlenmiyordu, bir doğrulayıcı kontrolü sabit bölündüğü gün ölmüştü,
bir soket testi yazılana kadar hiçbir şey görevleri süresiz tutan bir istemciyi
sormamıştı, ve bir test gerçek Keychain'de izin bekliyordu. Sonuncusu en
sessiziydi: tam koşu hiç bitmediği için kimse ne düştüğünü ne de geçtiğini
görüyordu. Hepsi yeşil raporluyordu — ya da hiç raporlamıyordu.

## 5. Karar bekleyenler

Kodla çözülmeyen maddeler. Cevaplanmadan planlanamazlar — sessizce varsayılan
seçmek, bu listenin var olma sebebine aykırı.

**Liste boş.** Beşinin dördü cevaplandı ve cevapları kod oldu; beşincisi
sorulduğunda **zaten kapatılmış** olduğu görüldü.

**Cevaplananlar:**

* *`latest.json` nerede yayınlanacak, anahtar nerede duracak (#2)?* — ADR 0025.
  **Aynı repoda GitHub Releases.** Endpoint artık `raw.githubusercontent.com`'daki
  bir dal dosyası değil, `releases/latest/download/latest.json` — yani
  `tauri-action`'ın `includeUpdaterJson` ile *zaten yazdığı* dosya. Eski adres
  iki bağımsız yönden yanlıştı: **sahip yanlıştı** (`stackvo/…`, oysa remote
  `fahrettinaksoy/…`) ve **mekanizma yanlıştı** (hiçbir şey o dosyayı `main`'e
  yazmıyor). `dialog: false` olduğu için ikisi de sessiz: updater 404 alıyor ve
  hiçbir şey söylemiyor. `updater_endpoint.rs` adresi `.git/config`'ten türetip
  karşılaştırıyor. Bu cevap **#21'i ve §2 C'nin anahtar törenini de** açtı.
* *Bir web yüzeyine kim bağlanabilir (#34)?* — ADR 0026. **Yalnız loopback, bir
  token, ve salt-okunur.** `websurface.rs` kararı çalıştırılabilir hâle
  getiriyor; taşıma katmanı **yok** ve bu eksiklik değil, sıralama: §5'in
  tuttuğu soru *ne servis edilir ve kime* idi.
* *Yerel AI servisleri (D-1)?* — ADR 0027. **Yalnız pgvector, ve bir servis
  olarak değil** — `postgres`'in bir sürümü. Bu depoda değişen kod: **hiç**, ve
  bu ADR 0011'in doğru çıkması.
* *`LEGACY_SERVICES` hangi sürümde siliniyor (#36)?* — **0.4.0.** İki minör
  boyunca göç desteklenir, sonra silinir. Tarih artık düzyazı değil bir kapı:
  `legacy_env_claims.rs` uygulama 0.4.0'a çıktığı ve sabit hâlâ orada olduğu an
  build'i kırıyor.
* *Kapsam eşiği* — **karar gerektirmiyordu, çünkü zaten verilmişti.** Satır
  "ölçüm var, kapı yok" diyordu; `tools/coverage-floors.mjs` tabanları
  kanıttan koyuyor (Rust satır %60, ön yüz %85/85/72),
  `tools/check-coverage.mjs` karşılaştırıyor ve CI'da "Hold the floors" adımı
  koşuyor. Satır kapı yazıldığında silinmemiş. Bu listenin kendi bakımı da
  §8'in sorusu.
* *Bir çalışma alanı kendi servis şablonunu beyan edebilir mi?* — ADR 0023.
  **Evet, ama bir servis olarak değil.** Depo bir **yan konteyner** beyan
  ediyor: projenin kendi compose bloğuna render ediliyor, projenin profiliyle
  kalkıp iniyor, `instances.json`'a hiç girmiyor. Host portu ve host yolu
  yok — ADR 0020'nin "konteyner zaten deponun kodunu çalıştırıyor" gerekçesi
  **yeni bir imaj için doğru değil**, o yüzden kapsama miras alınmadı, kuruldu.
* *İkinci bir arayüz (A-1)* — ADR 0017. Üçüncü yüzey kabul edildi, MCP'nin
  kabul edildiği şartla: her komut sözleşmedeki komutu adlandırıyor ve
  `cli_surface.rs` çifti kontrol ediyor.
* *Uygulama içi REPL yüzeyi (F-5)* — ADR 0022. `quickcmd.rs`'in gerekçesi geri
  **alınmadı**; reddettiği şeyin satır satır bir REPL olduğu, kabul edilenin ise
  düzenlenen bir parça kod olduğu ayrıldı. `tinker` hâlâ kullanıcının kendi
  terminalini açıyor.

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
etsin diye vardı. §5'in göç maddesi soruyordu: gömülü şablonlar silinince
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

### 0017 — Üçüncü yüzey kabul edildi, ama sözleşmeye bağlanarak

- **Status:** accepted
- **Context:** §5'in ikinci maddesi A-1'i kod eksikliğinden değil bir karardan
  bekletiyordu: bir CLI, masaüstü ve MCP'den sonra **üçüncü** bir tüketici
  demek, ve üçüncüsü `contracts/ipc.json`'dan sessizce ayrılabilecek üçüncü
  şey. E ve F suite'leri tam da bu kaymayı durdurmak için var. Maliyet gerçek;
  soru maliyetin ödenip ödenmeyeceğiydi.
- **Decision:** Kabul edildi, **MCP'nin kabul edildiği şartla**: `cli::COMMANDS`
  tablosundaki her komut, uyguladığı sözleşme komutunu adlandırıyor ve
  `tests/cli_surface.rs` çifti çapraz kontrol ediyor. Var olmayan bir komutu
  adlandıran satır build'i kırıyor; `mutation` bir komutun üstüne kurulup
  "Reads" başlığı altında listelenen bir satır da.

  Bir yer MCP tablosundan **daha sıkı**: orada bir araç *adına* göre dağıtılıyor,
  yani karşılığı olmayan bir tablo satırı derleniyor ve çağrıldığında düşüyor —
  modül bunu yazıp bir yedek dal bırakıyor. Burada tablo bir `Action` taşıyor,
  dağıtım enum üzerinde eşleşiyor, ve dalı olmayan bir varyantı derleyici
  reddediyor. "Listelenmiş ama uygulanmamış" diye bir durum test edilmiyor
  çünkü o duruma varılamıyor.

  Yüzeyin **aynı** olması şart değil ve olmamalı: `logs --follow` bir terminal
  cevabı, JSON-RPC üzerinde işe yaramaz. Şart olan, ikisinin ortak bir sözleşme
  komutu hakkında **yazıyor mu** konusunda anlaşması —
  `the_two_surfaces_agree_about_what_writes` bunu tutuyor, çünkü aksi hâlde
  ikisinden biri birine yalan söylüyor demektir.
- **Consequences:** ADR 0001'in bedeli burada tahsil edildi: `&Path` ve
  `&dyn ProgressSink` sayesinde tek bir domain fonksiyonu bile kopyalanmadı.
  ADR 0005'in bıraktığı boşluğa dördüncü sink geldi — `cli::Narrate`, stderr'e
  yazıyor; **stdout cevap, stderr anlatı**, yani `stackvo doctor --json | jq`
  build günlüğü akarken çalışıyor.

  Lifecycle yolundaki son Tauri bağı da koptu: `commands::run_hooks`'un gövdesi
  `hooks::run_for_project`'e taşındı, çünkü `AppHandle`'ı yalnızca sink'i
  kurmak için istiyordu. `stackvo stop` ile durdur düğmesi artık **aynı**
  hook'ları çalıştırıyor; ikinci bir kopya, tek isim taşıyan iki iş olurdu.

  Yazan komutlar aynı denetim izine düşüyor, `cli_` önekiyle: günlük "bu
  makineye ne oldu" sorusunu cevaplıyor ve "biri bunu terminalde çalıştırdı"
  cevabın bir parçası.

  Ve bir yüzey daha, gerçekten çalıştırılmadan bulunamayacak bir hatayı buldu:
  `db::targets`, `running`'i servis adına (`stackvo-mysql`) soruyordu, oysa
  konteyner instance tablosundan geliyor (`stackvo-mysql-9-7`). Ayakta olan dört
  veritabanı "kapalı" görünüyordu — ve dökme, geri yükleme ve anlık görüntü
  düğmeleri bu alana bakıyor. `db::instances` hemen üstünde doğrusunu yapıyordu.

### 0018 — Kabuk komutları sözleşmesiz, ve bu bir istisna değil bir sınır

- **Status:** accepted
- **Context:** A-3 (`stackvo php …`, `stackvo artisan …`) ADR 0017'nin kuralına
  çarpıyor: her CLI komutu `contracts/ipc.json`'daki bir komutu adlandırmalı.
  Bu komutların karşılığı **yok ve olamaz** — `quickcmd.rs`'in gerekçesi
  yüzünden: webview asla çalıştırılacak bir programı adlandıramaz, o yüzden
  sözleşmede program alan bir komut yok; `quickcmd_run` sabit bir katalogtan
  **id** alıyor.

  Üç yol vardı: (a) zorlama bir sözleşme komutu uydurmak, (b) bu komutları
  kapıdan muaf tutmak, (c) sınırı kaydırıp yeni yerini yazmak.
- **Decision:** (c). `cli::Backing` iki değer taşıyor: `Contract(ad)` ve
  `HostShell`. `HostShell` muaf **değil**, kendi kapısı var — `cli_surface.rs`
  dört şey doğruluyor: hepsi `docker exec` üzerinden geçiyor, hiçbiri host'ta
  program çalıştırmıyor, hepsi *yazan* olarak sınıflanmış, ve `--help`'te ilan
  edilen önek gerçekten çalışan argv.

  Gerekçe, `quickcmd.rs`'in gerekçesinin **kapsamı**: o kural bir *webview*
  hakkında — seçmediği kodu, yazmadığı sayfalardan çalıştıran bir şey. Terminal
  bunun tersi: yazan kişinin zaten bir kabuğu var, ve `stackvo artisan migrate`,
  onun yerine yazacağı `docker exec -it stackvo-shop php artisan migrate`'ten
  **daha az** tehlikeli — çünkü bu, konteyner adını yanlış yazamaz.

  **B-4 ile karıştırılmamalı** ve karıştırılması kolay: B-4 (§5.1) *çalışma
  alanının* diske yazılmış bir dosyayla beyan ettiği komut — bir depoyu
  klonlayan kişinin çalıştırdığı, yazarının seçtiği komut. Bu ise kullanıcının
  o an kendi klavyesinde yazdığı komut. İkisi arasındaki fark, kimin seçtiği;
  ve karar bekleyen o, bu değil.
- **Consequences:** Bayrak ayrıştırması kabuk komutunun adında **duruyor**.
  `stackvo artisan migrate --force` artisan'a bütün gidiyor; ayrıştırıcı okumaya
  devam etseydi `--force`'u yer ve sonra ondan şikâyet ederdi — artisan'ın en sık
  yazılan çağrısı. Bedeli: StackVo'nun kendi bayrakları komuttan **önce**
  yazılır, `--project` dahil. Bu yüzden `--project` global bir bayrak, ve bu
  yüzden `stackvo artisan --help` artisan'a gidiyor (`stackvo --help artisan`
  bu uygulamanınkini basıyor, ana yardım bunu söylüyor).

  Çıkış kodu aynen geçiyor: `stackvo artisan test` bir CI betiğinde, düşen bir
  test paketi 0 dönüyorsa hiçbir işe yaramaz.

  Çalışma dizini konteynerin içine eşleniyor — `app/Http`'de yazılan
  `stackvo artisan`, konteynerde `/var/www/html/app/Http`'de koşuyor. Yalnız
  kaynağı **mount edilen** projelerde: `generator.rs` PHP dışındaki runtime'lara
  kaynak mount'u yazmıyor (bir bind mount `/app`'i, yani derlenmiş çıktıyı
  gölgelerdi), o yüzden orada `-w` yok ve stderr'e tek satır uyarı düşüyor —
  `stackvo npm install` konteynerle birlikte kaybolan bir kopyaya yazıyor.

### 0019 — Bir ekran, kütüphanesi olmadan

- **Status:** accepted
- **Context:** M-8'in TUI yarısı için doğal cevap `ratatui`. Ölçüldü:
  `Cargo.lock`'a **25 paket** giriyor (649 → 674) — bir layout çözücü, bir
  widget kümesi, iki ayrı `unicode-width`, bir LRU önbellek, `strum`,
  `darling`, ikinci bir `rustix`. Bu ekranın çizdiği şey bir liste, bir detay
  satırı ve bir durum çubuğu.
- **Decision:** Kütüphane yok. Çizim `cli::Style` ve `cli.rs`'in tablolarında
  zaten kullanılan sütun aritmetiği; imleç, alternatif ekran ve renk birer
  ANSI dizisi, yani metin. İşletim sistemi gerektiren tek parça ham mod, ve
  iki yarısı da kilitte hazır: `libc` `portable-pty` üzerinden, `windows-sys`
  Tauri üzerinden. **Sıfır yeni paket** — ölçüldü, iddia değil.

  Girdi kendi thread'inde okunuyor. Ham modda stdin okuması bir tuş gelene
  kadar bloke eder, ekranın ise tuş gelmese de yenilenmesi gerekiyor;
  `poll`/`select` bunu Unix'te çözüp Windows için ikinci bir uygulama isterdi,
  bir thread ve bir kanal ikisinde de dokuz satırda çözüyor.
- **Consequences:** Bedeli `tui.rs`'in kendisi ve o bedel yazılı. Asıl risk
  kütüphane değil, **terminalin geri verilmesi**: ham modda bırakılan bir
  terminalde yankı yok, satır düzenleme yok, `Ctrl-C` çalışmıyor — ve kişi
  kurtulmak için körlemesine `reset` yazıyor. Dört çıkış yolunun dördü de
  kapalı: dönüş ve `?` için `Drop`; panic için bir hook (release'de
  `panic = "abort"`, yani `Drop` çalışmaz); `Ctrl-C` için tuş olarak okunması,
  çünkü ham mod onu sinyale çevirmeyi bırakıyor. Geri yükleme tek bir
  fonksiyondan geçiyor ve kayıtlı ayarları **alıyor**, böylece hook ile `Drop`
  ikisi birden ateşlense de bir kez çalışıyor.

  Ve bu okunarak değil **çalıştırılarak** tutuluyor: `examples/tui_probe.rs`
  gerçek bir pty açıyor, gerçek ikiliyi içinde koşturuyor, `j` ve `q`
  gönderiyor, ve terminalin kendi ayarlarını geri okuyup yankının ve satır
  modunun döndüğünü doğruluyor. Bu depoya bir kez ödettiği ders şuydu: bir
  kodlayıcı kendi beklentisine karşı sınandığında yalnızca yazarıyla
  hemfikir olur.

  `cli::Backing` üçüncü bir değer kazandı: `Surface(&[…])`. Bir ekran tek bir
  sözleşme komutunu uygulamıyor, birkaçını sürüyor — ve "hangisini uyguluyor"
  sorusunun dürüst tek cevabı yok. İsimlerin hepsi yine kontrol ediliyor, ve
  bir ekranın birden fazla ad taşıması testle şart koşuluyor: teki taşıyan bir
  satır zaten `Contract` olmalıydı.

### 0020 — Bir çalışma alanı kendi komutunu beyan edebilir, konteynerinin içinde

- **Status:** accepted
- **Context:** §5'in ilk maddesi B-4'ü bir karardan bekletiyordu. `quickcmd.rs`
  webview'in asla çalıştırılacak bir programı adlandıramayacağını savunuyor ve
  o gerekçe sağlam — ama gerekçe *webview* hakkında: seçmediği kodu, yazmadığı
  sayfalardan çalıştıran bir yüzey. Depoya işlenmiş bir dosya o yüzey değil.

  Karşı taraf da gerçek ve `hooks.rs`'in başlığında yazılı: bir depo klonlanır,
  açılır, düğmeye basılır — ve o depoyu yazanın seçtiği komutlar çalışır. Bu,
  kötü niyetli bir `package.json` `postinstall`'ıyla aynı şekil.
- **Decision:** Evet, **ama yalnızca konteynerin içinde.** `stackvo.json`
  `"commands"` taşıyor; her giriş bir id ve bir `exec` argv dizisi.
  `host` biçimi **yok**, ve yokluğu bu maddenin neden yeni bir onay akışı
  gerektirmediğinin tamamı: `hooks.rs`'in argümanı aynen geçerli — konteyner
  zaten deponun kodunu çalıştırıyor, orada komut çalıştırabilen bir depo yeni
  bir şey kazanmıyor. Bir **host** adımı ise `git clone` + düğmeyi keyfî kod
  çalıştırmaya çeviren şeydir ve onun digest'e bağlı bir rıza kaydı zaten var.

  Yani B-4 konteyner çizgisinde duruyor. Ötesine geçmek `hooks`'un `host`
  adımı: var, soruyor, ve **ayrı** bir karar.

  Üç kural daha, üçü de sessiz bir yanlışı imkânsız kılmak için:
  **argv dizisi, asla komut dizesi** — boşluktan bölmek `sh -c "a && b"`'yi
  dört argümana çevirir ve bu modülün tüm modeli kimsenin yeniden ayrıştırmadığı
  bir dizi olmasıdır; **gömülü bir id devralınamaz** — `migrate` diye beyan
  edilen bir komut reddediliyor, çünkü sessizce kazanması da kaybetmesi de aynı
  sonuca çıkar: biri `migrate` yazan bir düğmeye basar ve başka bir şey çalışır;
  **id dar** — küçük harf, rakam, tire, en fazla 40 karakter, çünkü id webview'e
  gidip geri geliyor ve o yolculukta kaçırılması gereken bir değer eninde
  sonunda kaçırılmayacaktır.
- **Consequences:** Yüzeyin sözü bozulmadı: webview hâlâ yalnızca bir **id**
  gönderiyor. Değişen, o id'nin nereden gelebileceği. `quickcmd::resolve` iki
  kaynağın birleştiği tek nokta ve `Resolved` tek şekil — bu çizginin altında
  beyan edilmiş bir komutu daha serbest davranabilecek hiçbir dal yok.

  Beyan edilen komut ekranda **işaretli** (`declared`), hem panelde hem
  `stackvo commands`'ta. Klonlanan bir depodan gelen satır, bu uygulamanın
  gönderdiği satırdan farklı bir şey, ve basıp basmamaya karar veren kişinin
  hangisine baktığını bilmeye hakkı var.

  Manifest serileştiricisi de öğrenmek zorundaydı, ve sebebi `hooks`'unkiyle
  aynı: bu metin her form kaydında yeniden yazılıyor, yani serileştiricinin
  bilmediği bir alan, biri alakasız bir ayarı değiştirdiği ilk anda sessizce
  kayboluyor. Bir projenin her gün çalıştırdığı komutu kaybetmesi, açılışta
  sessizce göç etmeyi bırakmasıyla aynı sınıf hata —
  `declared_commands_survive_the_editor_round_trip` bunu tutuyor.

  Ve bir cümle yanlış oldu, düzeltildi: `QUICK_COMMANDS_ARE_FIXED` ipucu
  "komutlar sabit katalogdan gelir" diyordu. Artık gelmiyor.

### 0021 — Güven zincirinin ilk halkası yazıldı; eksik olan bir anahtar, bir kod değil

- **Status:** accepted
- **Context:** `market.rs` zinciri üç halka olarak tarif ediyordu ve birincisi
  yoktu: *pinlenmiş anahtar → registry.json*. `Trust::Signed` uygulaması olmayan
  bir şekildi, `refresh` istendiğinde "uygulanmadı" diyerek reddediyordu. Yani
  C'nin "mimari **hazır**" cümlesi doğru değildi — hazır olduğu söylenen kapı
  kapanamıyordu.
- **Decision:** Doğrulayıcı yazıldı (`signing.rs`), **minisign** ile. Ölçüldü:
  `minisign-verify` Tauri'nin güncelleyicisi üzerinden **zaten `Cargo.lock`'ta**,
  yani sıfır yeni paket. minisign, ADR 0015'in istediği ed25519'un ta kendisi ve
  töreni yapacak araç (`minisign -G`) var — kendi aracını gerektiren bir şema,
  töreni hiç yapılmayan şemadır.

  **Resmî anahtar gömülmedi.** `PINNED` boş ve bir test onu boş tutuyor. Sahte
  bir anahtar koymak boşluktan kötü olurdu: sonraki her okuyucu zincirin
  kapandığına inanırdı. Anahtarsız bir derlemede imzalı tazeleme **reddediliyor**
  ve eksik olanın hangi yarı olduğunu söylüyor.

  **Kurumsal ayna beklemiyor.** Kendi indeksini imzalar, kendi anahtarını
  `policy.market.additionalKeys` ile pinler — o alan tam bunun için yazılmıştı
  ve bugüne kadar hiçbir okuyucusu yoktu. Üçüncü taraf dağıtımı böylece bir kod
  eksikliği olmaktan çıkıp bir işletme kararına dönüşüyor.

  **Rotasyon baştan var, çünkü sonradan eklenemez** (ADR 0015). Makine bir
  **küme** taşıyor; yeni anahtar, hâlihazırda güvenilen bir anahtarla imzalanmış
  bir `known-keys.json` ile tanıtılıyor. Yapamayacağı şey de kasıtlı: sızmış bir
  anahtar yalnızca kendini adlandıran bir belge imzalayabileceği için,
  **emeklilik bir derlemedir** — `RETIRED`'daki bir anahtar, onu adlandıran belge
  ne kadar geçerli imzalanmış olursa olsun geri gelmiyor, ve politika da onu geri
  getiremiyor.

  **Kaldırmanın istemci yarısı** iki parça: geri çekilmiş bir sürüm **kurulmuyor**
  (uyarı değil, ret — ADR 0014 sürümü indekste tutuyor ki makine ne olduğunu
  öğrenebilsin, ama yeni kurulumun devam edip etmeyeceği ayrı bir soru), ve
  **zaten kurulmuş** olanı `doctor` bildiriyor. İkincisi olmadan birincisi
  yarımdı: konteyner çalışmaya devam eder, yığın sağlıklı görünür, geri çekilme
  kimsenin elle okumadığı bir indeks satırında kalır.
- **Consequences:** İki karar yolda değişti, ikisi de yazılarak.

  `allow_legacy` önce `false` idi — "iki mod farklı şey imzalıyor, ikisini de
  kabul etmek biri üzerine yapılmış imzayı öteki için geçerli kılar" diye. Bu
  yanlıştı: mod imza dosyasında beyan ediliyor, doğrulayıcı ona göre hash'liyor
  ya da hash'lemiyor, ve birini öteki gibi sunmak yalnızca doğrulamayı düşürüyor.
  Reddetmek hiçbir şey kazandırmıyor, ama eski bir `minisign` ile imzalanmış
  kurumsal aynayı sebebi anlaşılmaz bir mesajla reddediyordu.

  Ve sıralama: anahtar kontrolü, imza dosyasını getirmeden **önce** yapılıyor.
  Önce getiriyordu, ve anahtarı olmayan bir makineye
  `registry.json.minisig: No such file` diyordu — eksik yarısı kendi tarafındayken
  insanı yayıncıdan imza istemeye gönderen bir cümle. Sıralamayı iddia eden bir
  test bulmuştu, okumak değil.

  Pozitif yol gerçek bir imzaya karşı sınanıyor ve vektör `minisign-verify`'ın
  kendi testinden alındı — kendi ürettiği bir çiftle sınanan bir doğrulayıcı,
  yalnızca minisign'ın ne olduğuna dair kendi fikriyle hemfikir olur. Bu depoya
  o dersi QR kodlayıcı bir kez ödetmişti.

### 0022 — Uygulama içi tezgâh kabul edildi; reddedilen şey satır satır REPL'di

- **Status:** accepted
- **Context:** `quickcmd.rs` uygulama içi bir REPL panelini **yazılı olarak**
  reddetmişti: "zaten yapılandırdıkları REPL'in yanında ikinci ve daha kötü bir
  REPL". §5 bunu (o zamanki beşinci madde olarak) bir görev değil bir **karar**
  olarak tutuyordu, çünkü yazılı
  bir reddi bir commit sessizce geri alamaz.
- **Decision:** Ret **doğru** ve yerinde duruyor — satır satır bir REPL için.
  `tinker` hâlâ kullanıcının kendi terminalini açıyor. Kabul edilen şey farklı
  bir alet: bir **parça kod**, düzenlenen ve yeniden çalıştırılan yirmi satır.
  Terminaldeki REPL'de bu yeniden yazmaktır; burada metin kalır. İkisi
  sıralanmıyor, kişinin ne yaptığına göre ayrılıyor — ve ekranda da öyle
  duruyorlar: panel, terminal panelinin hemen altında.

  Güvenlik modeli `quickcmd`'inkinin bir seviye altına genişletiliyor, kırılmıyor:
  webview bir **çalıştırıcı kimliği** gönderiyor, programı yine adlandırmıyor
  (`laravel` = `php artisan tinker --execute`, başka bir şey değil). Kod **tek
  bir argv elemanı** ve her zaman **sonuncusu**; hiçbir yerde kabuk yok. Onay
  kapısı gerekmiyor, çünkü kod (a) projenin **kendi konteynerinde** çalışıyor —
  o konteyner zaten o deponun kodunu çalıştırıyor, `hooks` gerekçeyi tam olarak
  yazıyor — ve (b) klonlanmış bir dosyadan değil, **klavyedeki kişiden** geliyor.
  `host` çalıştırıcısı yok ve olmayacak: bu makinede çalışması gereken bir adım
  `hooks`'un digest'e bağlı `host` adımıdır.
- **Consequences:** İki sınır kodda, ikisi de ölçülerek.

  **Süre sınırı konteynerin içinde.** Bir `docker exec` **istemcisini**
  öldürmek, başlattığı süreci durdurmuyor — yani yalnızca uygulamanın kendi
  saatiyle "zaman aşımı" demek, birinin konteynerinde CPU yakmaya devam eden bir
  döngüyü sessizce bırakmak olurdu. Komut `timeout 30` ile önden sarılıyor;
  `timeout` php, node, python, ruby ve wordpress-cli imajlarında ve bu çalışma
  alanının kendi proje konteynerinde ölçüldü. "Baktığım her imaj" "her imaj"
  olmadığı için imajda yoksa geri düşülüyor ve `limited` alanı hangisinin
  olduğunu **söylüyor**.

  **Başarı çıkış kodundan okunuyor, "stderr boş mu"dan değil.** PHP'nin ölümcül
  hatası **stdout**'a yazılıyor (ölçüldü), Node'unki stderr'e. İki akış da
  gösteriliyor; yalnızca birini gösteren bir panel, sunduğu dillerin yarısında
  boş kalırdı.

  Geçmiş **kodu saklıyor, çıktıyı değil**: parça kod kişinin kendi yazdığıdır ve
  geri istediği şeydir, çıktı ise **uygulamanın verisidir** — `querylog`'un
  koyduğu kural. Dosya uygulamanın kendi yapılandırma dizininde, projenin içinde
  değil; bir checkout'a yazılan dosya birinin `git status`'ünde beliren dosyadır.


### 0023 — Bir depo yan konteyner beyan edebilir, servis değil

- **Status:** accepted
- **Context:** §5'in ilk maddesi iki yarımdı. *Komut* yarısı ADR 0020 ile
  cevaplandı ve gerekçesi tek cümleydi: projenin konteyneri **zaten** deponun
  kodunu çalıştırıyor, dolayısıyla depoya orada komut adlandırma izni yeni bir
  şey kazandırmıyor. Kalan yarı bir *servis* tanımıydı, ve o cümle oraya
  taşınmıyor: yan konteyner **başka bir imaj**. `typesense/typesense:27.1`'in
  bu makinede çalışması, dosya öyle demeden önce doğru değildi. Yani kapsama
  miras alınamazdı; kurulması gerekiyordu.

  İkinci sorun kapsamdı. `instances.json` proje alanı taşımıyor: instance'lar
  yığın çapında, portlar host çapında ayrılıyor. Yığın çapında bir depo-tanımı,
  aynı depoyu iki kez klonlayan herkes için bir port ve bir volume çakışması
  demekti — nadir değil, rutin.
- **Decision:** Evet, **bir yan konteyner olarak.** `stackvo.json` `"sidecars"`
  taşıyor; her giriş bir imaj (etiketli), isteğe bağlı argv, env ve adlandırılmış
  volume'ler. Kapsam üç kuralla kuruluyor, üçü de kod:

  **Host portu yok, host yolu yok.** Beyan edilen konteyner projeden erişilebilir,
  başka hiçbir şeyden değil; mount'lar yalnız Docker volume'leri. `ports` yazan
  ya da `/` ile başlamayan bir `path` veren bir giriş **adıyla reddediliyor** —
  yok sayılmıyor, çünkü onu yazan kişinin düzeltilecek bir modeli var.

  **Her ad türetiliyor, hiçbiri beyan edilmiyor.** Konteyner
  `stackvo-<proje>-<id>`, volume `stackvo-<proje>-<id>-<handle>`. Dosyadaki
  hiçbir şey global bir ad seçmiyor, dolayısıyla dosyadaki hiçbir şey
  başkasınınkiyle çakışamıyor. Compose anahtarı da id değil konteyner adı: iki
  proje `search` derse, tek anahtar iki kez yazılmış olurdu ve ikincisi
  sessizce kazanırdı.

  **Projenin profiliyle yaşıyor.** Projenin kendi compose bloğuna, projenin
  profiliyle render ediliyor — `--profile project-shop` ikisini de kaldırıyor,
  shop durunca yan konteyner de duruyor. Yeni bir mekanizma değil, zaten
  kullanılan mekanizma.

  **Bir servis değil**, ve ayrım maddenin tamamı: `services: ["mysql"]` bir
  *katalog id'si* — bir ihtiyaç, makinenin bir paketten karşıladığı, isteyen
  her projenin paylaştığı tek instance. Yan konteyner bunun tersi şekli:
  `instances.json`'da yok, çözülecek sürümü yok, kurulacak paketi yok,
  markette yeri yok, paylaşılmıyor. İkisini tek listeye koymak "bunlardan kaç
  tane var" sorusunu iki cevaplı yapardı.
- **Consequences:** Serileştirici üçüncü kez öğrenmek zorundaydı ve sebebi
  `hooks` ile `commands`'ınkiyle aynı: bu metin her form kaydında yeniden
  yazılıyor, yani serileştiricinin bilmediği bir blok, biri alakasız bir ayarı
  değiştirdiği ilk anda sessizce kayboluyor —
  `declared_sidecars_survive_the_editor_round_trip` bunu tutuyor.

  Reddedilen yarı **"asla" değil**: host portu ya da host dizini isteyen bir
  beyan, `hooks`'un şeklinde — digest'e bağlı, depo başına bir kez sorulan ve
  beyan değiştiğinde yeniden sorulan — bir onay kapısının arkasına ait. O kapı
  henüz yok, ve olmadığı sürece dürüst hâl reddetmek. Reddin *ayrıştırma*
  zamanında olması da bunun parçası: mesaj manifest'in üstünde beliriyor, bir
  compose dosyasının içinden değil.

  Etiketsiz imaj da reddediliyor, ADR 0014'ün `latest` hakkında söylediğiyle
  aynı sebeple: etiketsiz bir imaj geçen ay çekmiş birinin altından kayıyor, ve
  kendi PHP sürümünü pinleyen bir deponun bunu serbest bırakmasının sebebi yok.

### 0024 — Sözleşme, tel üzerindeki adı yazar; Rust'ınkini değil

- **Status:** accepted
- **Context:** §3 #10 sözleşmenin `types` tablosunu ilk kez *tüketen* şeyi
  yazdı — `src/lib/ipc.d.ts`. Tablo o güne kadar hiçbir şey tarafından
  okunmamıştı, ve okunur okunmaz yanlış olduğu görüldü.
- **Decision:** `contracts/ipc.json`'daki her alan adı, bir yükün gerçekten
  taşıdığı ad olmak zorundadır — yani camelCase. Rust'ın alan yazımı bu dosyaya
  girmez. `contract_spelling.rs` iki şeyi birden tutuyor: dokümanın yazımını, ve
  o yazımın neden doğru olduğunu. Komut adları (`pty_open`) ve olay adları
  (`build:start`) bunun dışında — onlar birer tanımlayıcı, alan değil.
- **Consequences:** Sözleşmeye eklenen her yeni alan, koddan okunarak
  yazılmak zorunda; tahminle yazılan bir ad artık build'i kırıyor. Yeni bir
  `pub struct` de `rename_all` taşımak zorunda, yoksa ilk kuralı sessizce
  yanlışlar. Bir **dosyayı** yansıtan yapı (tel değil) `FILE_SHAPES`'te adıyla
  ve gerekçesiyle muaf tutuluyor — bugün bir tane var. `ipc.d.ts` yeniden
  üretildi ve `npm run types:tsc` ile derlenmesi CI'da tutuluyor.

**Bu bir düzeltme, bir tercih değil.** Sözleşme **106 alan adını** Rust'ın
yazımıyla taşıyordu — `managed_by_stackvo`, `operation_id`, `cpu_percent`,
`session_id`, `install_ca`, `exit_code`. Tel hiçbirini taşımadı, bir kez bile:
bir komutun döndürdüğü her yapı `#[serde(rename_all = "camelCase")]` türetiyor
ve Tauri argümanları girişte camelCase'e çeviriyor. Ön yüz baştan beri
`managedByStackvo` okuyordu. Yanlış olan kod değil, **kodu anlatan belgeydi**.

**Neden kimse görmedi.** Tip tablosunu tüketen hiçbir şey yoktu.
`contract_agreement.rs` komut *kümesini* koda karşı denetliyor;
`contract_version.rs` sözleşmeyi *kendi önceki kopyasına* karşı denetliyor —
ikincisi bir yanlışı sadakatle korur, çünkü referansı yine kendisidir. Aradaki
boşluk tam olarak şuydu: **hiçbir kapı sözleşmeyi Rust'a karşı okumuyordu.**
§3 #10 o tabloyu `src/lib/ipc.d.ts`'e çevirir çevirmez boşluk kapandı ve aynı
anda pahalıya döndü — yanlış bir ad, eksik bir addan kötüdür, çünkü bir editör
onu **olgu olarak tekrar eder**.

**İkinci iddia birincisini taşıyor.** "Hepsi camelCase" cümlesi ancak her
serileştirilebilir yapı yeniden adlandırıldığı sürece doğru; `rename_all`
taşımayan tek bir yapı, sözleşmede tek satır değişmeden kuralı yanlışlar. Bu
yüzden ikisi de test: 204 yapı taranıyor, tek istisna `ExtensionSpec` ve o da
bir **dosyayı** yansıtıyor (`contracts/php-extensions.json` snake_case yazılmış),
tele hiç çıkmıyor — `catalog_get` `ExtensionOption` ile cevap veriyor.

**Sürüm.** Bu düzeltmeler ADR 0008'in mekanik kuralına göre *kırıcı* sayılıyor
(alan kaybı + alan kazancı), ve bu doğru sonucu veriyor: `surface.lock.json`
1.0.0'da, sözleşme 2.0.0'da, yani gereken artış zaten yapılmış durumda. Ayrıca
`git tag` boş — **hiçbir sürüm hiç yayınlanmadı**, dolayısıyla kırılacak bir
istemci yok. Yayınlanmamış bir sözleşmedeki bir yazım hatasını düzeltmenin
maliyeti tam olarak sıfır; yayınlandıktan sonra düzeltmenin maliyeti bir majör
sürüm ve bir göç notu olurdu. Bunu şimdi yapmanın sebebi bu.

---

### 0025 — Güncelleme aynı repodan, release varlığı olarak

- **Status:** accepted
- **Context:** §3 #2 "endpoint 404 veriyor" diyordu ve bir mühendislik maddesi
  gibi duruyordu. Değildi: `latest.json`'ın nerede yayınlanacağı ve özel
  anahtarın kimde duracağı bir karardı, ve **#21 (sürüm kanalları) ile §2 C'nin
  anahtar töreni de** o kararın arkasında bekliyordu.
- **Decision:** `latest.json` bu deponun kendi GitHub Releases'inde yayınlanır;
  endpoint `https://github.com/<owner>/<repo>/releases/latest/download/latest.json`
  olur. Özel anahtar `TAURI_SIGNING_PRIVATE_KEY` repository secret'ı olarak
  eklenir, açık yarısı `tauri.conf.json`'daki `pubkey`'de kalır. Ayrı bir
  dağıtım reposu **reddedildi**: ikinci bir CI ve iki yerin sürüm numarasını
  eşit tutma işi, bugünkü tek yazarlı depoda ayrı bir sahiplik sınırının
  getirisini karşılamıyor — getirisi olduğu gün taşınabilir, endpoint zaten
  türetiliyor.
- **Consequences:** Eski adres **iki bağımsız yönden** yanlıştı ve ikisi de
  sessizdi. Sahibi yanlıştı — `stackvo/stackvo-tauri`, oysa remote
  `fahrettinaksoy/stackvo-tauri`; bir sürüm yayınlamak bunu düzeltmezdi.
  Mekanizması da yanlıştı — `raw.githubusercontent.com/.../main/latest.json`,
  ve o dosyayı `main`'e yazan hiçbir şey yok; `tauri-action` onu **release'in
  içine** yazıyor. `dialog: false` olduğu için updater 404 alıp hiçbir şey
  söylemiyor. Tek başına her biri, "düzeltildi" denip hâlâ bozuk kalabilecek
  bir hataydı. `updater_endpoint.rs` adresi `.git/config`'ten türetip
  karşılaştırıyor, ve `includeUpdaterJson` ile imzalama secret'ının hâlâ
  workflow'da olduğunu tutuyor — üçü üç ayrı dosyada ve hiçbiri ötekini
  anmıyordu. Ağ erişimi yok: URL'in *cevap verdiği* denenmiyor, çünkü GitHub
  yavaşken kızaran bir kapı insanların görmezden gelmeyi öğrendiği kapıdır.

---

### 0026 — Web yüzeyi: yalnız loopback, bir token, ve salt-okunur

- **Status:** accepted
- **Context:** §3 #34 §5'te duruyordu çünkü bir HTTP yüzeyi **bu** komut
  kümesini bir sokete koymak demek, ve bu kümede `quickcmd_run`,
  `project_hooks_approve`, `env_reveal` ve `elevate` destekli yollar var.
  Cevapsız yazılan bir sunucu bir özellik değil, changelog girdisi olan bir
  uzaktan kod çalıştırma yüzeyidir.
- **Decision:** `127.0.0.1`, koşu başına üretilen bir token, ve yalnızca
  okumalar. Dördüncü kural üçünü taşıyor: `kind: "query"` **açılabilir demek
  değil** — `instance_reveal` bir sorgudur ve keystore'dan parola döndürür.
  Bir komut ancak bir sorguysa **ve** kod yolu keystore'a ulaşamıyorsa
  servis edilir.
- **Consequences:** `websurface.rs` kararı saf mantık olarak taşıyor: `exposable`,
  `admit` (token **önce** kontrol edilir — komut adı önce bakılsaydı, token'ı
  olmayan birine komut envanteri verilirdi) ve sabit-zamanlı `token_matches`
  (boş beklenen token kimseyi kabul etmez; `"" == ""` doğrudur ve yüzeyi açardı).
  Taşıma katmanı sonradan yazıldı ve o sıra kasıtlıydı: §5'in tuttuğu soru *ne servis edilir ve kime* idi, ve o cevaplanmadan yazılmış bir dinleyici satırın uyardığı şeyin ta kendisi olurdu.

  İkinci kuralın listesi elle yazıldığında **dördünün üçü yanlıştı** —
  `stripe_status` ve `service_connection` tahmindi, `instance_settings` ve
  `secrets_status` kaçmıştı. "Bu parola döndürür mü" kaynak metnin cevapladığı
  bir soru değil; cevapladığı soru bir çağrı yolunun var olup olmadığı.
  `websurface_claims.rs` onu tüm crate üzerinde bir **fixpoint** olarak
  hesaplıyor, ve üç kez düzeltmek gerekti: tek sıçrama `service_reveal`'i
  kaçırıyordu (üç satır, `instance_reveal`'e devrediyor, o da yardımcıya);
  çıplak isimle kurulan graf **112 sorgunun 101'ini** reddetti (`read`, `load`,
  `of` bir düzine modülde var — güvenli yön, ama on bir komut servis eden bir
  yüzey de yüzey değil); doğrusu modülle nitelenmiş kenarlar oldu. Sonuç:
  112 okumanın 15'i reddediliyor. Reddi *kanıtlanana* göre adlandırmak da
  bunun parçası — sabit `REACHES_THE_KEYSTORE`, `READS_A_SECRET` değil.

---

### 0027 — Yerel AI: yalnız pgvector, ve bir sürüm olarak

- **Status:** accepted
- **Context:** §2'nin D-1 satırı Ollama, Qdrant ve pgvector'ün katalog servisi
  olup olmayacağını soruyordu; §5'te "ertelendi" olarak duruyordu.
- **Decision:** Yalnız pgvector, ve **bir servis olarak değil** — `postgres`'in
  bir sürümü (`16-pgvector`, `pgvector/pgvector:pg16`). Ollama ilk koşuda 4–8 GB
  model çekiyor ve bulamayabileceği bir GPU istiyor; Qdrant zaten dört olan
  veritabanlarının beşincisi. İkisi de bu kataloğun ölçeğinde değil, ve
  "Laradock'un 130 servisi" zaten girilmeyecek kavgalar arasında yazılı.
- **Consequences:** **Bu depoda değişen kod: hiç** — ve bulgunun kendisi bu.
  ADR 0011 uygulamanın hiçbir servis tanımı taşımamasını kararlaştırmıştı,
  dolayısıyla yeni bir servis bir *paket*; uygulamanın zaten ifade edebildiği
  bir paket burada sıfıra mal oluyor. Ayıran şey bir **capability**: `vector`.
  `commands.rs` bir `instanceRef`'i tam olarak böyle çözüyor, yani mekanizma
  vardı. Sabitlenen sürüm de `recommendedVersion` **değil**: PostgreSQL kuran
  PostgreSQL almalı, ve fazladan bir eklenti taşıyan imaj daha iyi bir
  varsayılan değil. `vector_capability.rs` cümleyi tekrar etmiyor, kanıtlıyor —
  fixture gerçek bir paket ve capability yolu ikisini ayıramasaydı cevap yanlış
  olurdu. Ollama ya da Qdrant isteyen `sidecars` yazar; ADR 0023 tam da bunu
  mümkün kıldı, ve hayır demenin bir ret olmamasının sebebi bu.

---

### 0028 — Bir kapı, koştuğu kanıtlanana kadar bir kapı değildir

- **Status:** accepted
- **Context:** §3'ün dört maddesi (#12, #22, #33, #35) "CI dışında koşamıyor"
  diyordu ve bu bir ortam sorunu gibi okunuyordu. Koşturulunca dördünün de
  altında aynı şey çıktı.
- **Decision:** Bir gate'in **koştuğu** kanıtlanmadan yeşilliği delil sayılmaz.
  Pratikte üç kural: (a) bir doğrulayıcının **kendi girdisi** olur — yanında ne
  bulunduğuna bağlı olamaz; (b) bir süit, kaç testinin *koştuğunu* da raporlar,
  yalnız kaçının geçtiğini değil; (c) yalnız CI'da koşabilen bir şey, CI dışında
  koşabilir hâle getirilir (`tools/linux/`).
- **Consequences:** Dört bulgu, dördü de yeşil raporluyordu. Suite A hiçbir
  makinede **bir kez bile** manifest okumamıştı — `tools/fixtures/validator-workspace/`
  ona kendi girdisini verdi ve iki ölü kontrol çıktı: `EMBEDDED` kazıyıcısı
  sabit ikiye bölündüğü gün eşleşmeyi bırakıp **boş küme** döndürüyordu, ve
  onarıldığında da yalnız anahtar adlarını okuyordu (uyarılar 21 → 1). Driver
  süiti **hatayı SKIP olarak** raporluyordu — `node:test`, `null` bir `skip`'i
  direktif okuyor — ve dört gerçek düşüşle yeşil çıkıyordu; CI'ya artık
  "hiçbiri atlanmadı" adımı da eklendi. Bu depo **Linux'ta hiç derlenmiyordu**:
  `certs.rs`'in `not(macos)` dalı var olmayan bir fonksiyonu çağırıyordu, yani
  CI'nın Linux `build` job'ı kırmızıydı. Ve driver süitinin manşet testi kendi
  seçtiği derleme profiliyle **geçemezdi** — `cargo build` `devUrl`'i gömüyor.
  `cfg_regions.rs` üçüncüsünün sınıfını, `release_rehearsal.rs` koşucusuz
  tutulabilen iddiayı tutuyor.

---

### 0029 — Yerel API varsayılan kapalı, ve token bir kez gösterilir

- **Status:** accepted
- **Context:** ADR 0026 *ne servis edilir ve kime* sorusunu cevapladı. Taşıma
  katmanı yazılınca iki soru daha çıktı ve ikisi de kod değil varsayılan
  sorusuydu: yüzey ne zaman ayakta olacak, ve token'ı kim görecek.
- **Decision:** **Varsayılan kapalı**, ve Ayarlar'daki bir panelden açılıyor.
  Token `websurface_start`'tan **bir kez** dönüyor; diske hiç yazılmıyor ve
  `websurface_status` onu taşımıyor. Kaybedilirse durdur-başlat.
- **Consequences:** Loopback'in kendisi tehlikeli olduğu için değil: **haberi
  olunmayan bir dinleyici, kimsenin kapatmadığı dinleyicidir**, ve birinin
  çalışma alanı hakkındaki soruları cevaplayan bir yüzey için dürüst varsayılan
  cevaplamıyor olmasıdır. Token'ın statüde taşınmaması bir titizlik değil bir
  gereklilik: taşısaydı sonraki her çağırana verilirdi, ve bunların **ilki
  yüzeyin kendisi** olurdu — salt-okunur bir API, kendi anahtarını dağıtan bir
  API hâline gelirdi. Diske yazmamak da aynı cümlenin devamı: bir dosyadaki
  token, onu üreten süreçten uzun yaşar. Bedeli gerçek ve kabul edildi —
  uygulama yeniden yüklenince token kayboluyor; panel bunu ekranda söylüyor,
  keşfedilmeye bırakmıyor. İkinci bir `start` çakışma sayılıyor: sessizce
  öncekinin adresini döndürmek, çağırana hiç görmediği bir token'ı ait
  gösterirdi.

---

## 7. Ölçüm

Mekanik olarak sayılabilenler koda karşı tutuluyor:
`src-tauri/tests/platform_matrix_claims.rs` yanlış bir sayıda build'i kırıyor.

| | Sayı | Nasıl sayıldı |
|---|---|---|
| Toplam IPC komutu | **253** | `contracts/ipc.json` → `commands` (250 Rust + 3 `frontend-plugin`) |
| Bunlardan `#[tauri::command]` olarak yazılmış | **249** | `commands.rs`, `#[cfg(test)]` dışı |
| Frontend kaynak dosyası | **134** | `src/**/*.{js,vue}`, spec dosyaları hariç |
| Bunlardan `@tauri-apps` kullanan | **20** | aynı küme içinde metin taraması |
| **Veri katmanının geçtiği fonksiyon** | **1** (`src/lib/ipc.js` → `call()`) | `invoke(` `ipc.js` dışında **0** yerde geçiyor |
| `ipc.js` sarmalayıcısı | **246** | `api` nesnesinin üye sayısı |
| Rust kaynağı | **97 modül, 85.774 satır** | `src-tauri/src/*.rs` |
| Gömülü varsayılan — **kalan** | **36** | `config.rs` → `SETTINGS` |
| Gömülü varsayılan — **yalnız göç için** | **150** | `config.rs` → `LEGACY_SERVICES`; toplam **186** |

Son satır iki yerden okunuyor ve bir süre yalnız birinden okunuyordu:
`tools/validate-contracts.mjs`'nin kazıyıcısı düz bir dizi bekliyordu, sabit
ikiye bölününce eşleşmeyi bıraktı ve **boş küme** döndürdü. Bir metin
kazıyıcısının başarısızlık biçimi budur — hiçbir şey bulmak, ve sakin
görünmek — o yüzden artık bir taban var (`EMBEDDED_UNREADABLE`).

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

1. **§5'teki karar tablosu ve §7'deki ölçüm testlerle tutuluyor.** Bir karar
   Status/Decision/Consequences taşımazsa, ya da bir sayı ağaçla uyuşmazsa,
   build kırılır (`architecture_claims.rs`, `platform_matrix_claims.rs`,
   `policy_claims.rs`, `secrets_claims.rs`).
2. **§2–§4 kapıya bağlanamaz** — "yapılmadı" ölçülemez. Elde olan tek şey her
   satırın **nasıl bakıldığını** taşıması; bir sonraki oturum tabloyu okumak
   yerine aynı kontrolü tekrarlayabilir.
3. **Bir madde bittiğinde satırı buradan silinir** ve kaydı `CHANGELOG.md`'ye,
   geri alınamaz bir tercih taşıyorsa §6'ya yazılır. Bir sonraki okuyucunun
   ihtiyaç duyduğu şey ne yapıldığı değil, neden öyle yapıldığı.
