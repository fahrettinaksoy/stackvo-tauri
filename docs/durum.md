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
| D-1 | Yerel AI servisleri (Ollama, Qdrant, pgvector) | 🔒 | §5.2'de **ertelendi** olarak kayıtlı, kapsam dışı değil |

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
| 2 | Güncelleme endpoint'i | ⛔ | `tauri.conf.json` `latest.json`'a işaret ediyor → HTTP 404; repo yok. §5.3'teki sahiplik kararı |
| 10 | `tauri-specta` ile tip üretimi | ⬜ | `specta`/`ts-rs`/`typeshare` `Cargo.toml`'da yok. ADR 0006 ölçtü ve erteledi |
| 12 | E2E | 🟡 | **Webview yarısı indi ve genişledi**: Playwright gerçek motorda — 6 kabuk, **dokuz rotanın hepsinde** axe (tüm belge, `#app` kapsamı kaldırıldı) ve 2 RTL yerleşim testi; CI'da adım var. `tauri-driver` yarısı yok ve **bu makinede olamaz** — Tauri'nin belgesi macOS'u desteklemediğini yazıyor. Beyanın ihtiyaç duyduğu ölçüm artık burada (`docs/accessibility.md`) |
| 21 | Sürüm kanalları, kademeli dağıtım, geri alma | ⛔ | `tauri.conf.json`'da `channel`/`rollout`/`paused` yok; #2'nin arkasında |
| 22 | Platform kapsamı (Linux aarch64, Win ARM64) | 🟡 | `release.yml` artık **altı** hedef sayıyor: iki ARM satırı, çapraz derleme yerine yerel ARM koşucularıyla (`ubuntu-24.04-arm`, `windows-11-arm`), ve OS koşulları aile testine çevrildi. **Bir etiket koşana kadar doğrulanmadı** — bir geliştirici makinesinde koşucu etiketinin var olduğu da paketleyicinin orada mutlu olduğu da kanıtlanamaz; `fail-fast: false` yanlış bir tahminin diğer dördü düşürmesini engelliyor |
| 31 | Air-gapped kurulum | 🟡 | Gidiş-dönüş tam ve arayüzde (`market.offlineBundle`, `CatalogueGate.vue`); paket yolu yok |
| 33 | Sözleşme kapısının harici bağımlılığı | 🟡 | Checkout var ama **suite A hiç koşmuyor** — bu makinede de `NO_MANIFESTS` (`tools/validate-contracts.mjs`) |
| 34 | Web sürümü / HTTP ikilisi | ⬜ | `src-tauri/src/bin/` iki ikili taşıyor (`stackvo-mcp`, `stackvo`) ve ikisi de HTTP konuşmuyor: biri stdio, öteki argv. §7: veri yolu tek fonksiyondan geçiyor, karşılığı olmayan dört komut orada adlandırılmış. **M-8'in PWA yarısı buna bağlı** ve tek kaydı burası — dayanacağı HTTP yüzeyi çıkmadan PWA bir madde değil, bunun sonucu |
| 35 | Windows ve Linux dallarının çalıştırılması | 🟡 | **Hosts yarısı üç OS'ta da koşuyor**: `hosts_path()` `STACKVO_HOSTS_PATH` dikişini tanıyor ve `tests/hosts_roundtrip.rs` planı, yazmayı, geri okumayı ve kaldırmayı geçici bir dosyada baştan sona koşuyor. Bunu mümkün kılan değişiklik kendi başına da bir düzeltme: `apply` artık **yazabiliyorsa parola sormuyor**, yalnızca yazamadığında yükseliyor. Kalan yarı **yükseltmenin kendisi** — pkexec/UAC/osascript diyaloğu bir insan gerektiriyor |
| 36 | `EMBEDDED`'ın servis yarısı | ⬜ | ADR 0016'dan sonra **yalnız göç için** duruyor: `handover` `.env`'i okuyor ve `SERVICE_*_ENABLE/VERSION` varsayılanlarına ihtiyaç duyuyor. Desteklenen hiçbir çalışma alanı göç bekler durumda kalmayınca gitmeli — `config.rs`'te 186 anahtarın yaklaşık yarısı |

---

## 4. Önerilen sıra

Karar gerektirmeyenler arasından, etki ÷ efor ile.

1. **#12'nin kalanı: `tauri-driver`, Linux CI'da.** Webview yarısı indi,
   #25'in beklediği ölçümü verdi ve o ölçüm dokuz rotaya genişledi. Kalan yarı
   bir CI işi, bir masaüstü işi değil — ve macOS'ta *yazılamaz* değil,
   **koşulamaz**: yazan kişi geçtiğini hiç göremez.
2. **#36: `EMBEDDED`'ın servis yarısı.** Göç bitip desteklenen kurulum
   kalmayınca 186 anahtarın yarısı gidiyor.
3. **#22 ve #35: platform kapsamı ve ayrıcalık yollarının koşulması.** CI işi.
4. **#31: air-gapped paket yolu.** Gidiş-dönüşün kalan ucu.

Karar bekleyenler (§5) ve dışarıdan bir şey gerektirenler (#2, #21, #33) bu
sıraya girmiyor.

---

## 5. Karar bekleyenler

Kodla çözülmeyen maddeler. Cevaplanmadan planlanamazlar — sessizce varsayılan
seçmek, bu listenin var olma sebebine aykırı.

1. **Bir çalışma alanı kendi servis şablonunu beyan edebilir mi?** Sorunun
   *komut* yarısı ADR 0020 ile cevaplandı: evet, kendi konteynerinin içinde
   kalmak şartıyla. Bu kalanı ve **C ile karıştırılmamalı** — C üçüncü
   tarafların paket *yayınlaması*, bu ise tek bir çalışma alanının kendi
   servisini *tarif etmesi*. Ayrı bir soru olmasının sebebi kapsam: bir komut o
   projenin kendi konteynerinde koşuyor, bir servis tanımı ise yığının
   tamamına giriyor ve bir imajı, bir portu ve bir volume'ü adlandırıyor.
2. **Yerel AI servisleri (D-1).** **Ertelendi** olarak kayıtlı, kapsam dışı
   değil. Ollama, Qdrant ve pgvector birer katalog servisi olsun mu — kapatılan
   LLM-gateway sorusundan farklı bir soru.
3. **Güncelleme endpoint'i ve imzalama secret'ları (#2).** `latest.json` nerede
   yayınlanacak: `stackvo/stackvo` release'leri mi, yeni bir repo mu? Özel
   anahtar `~/.tauri/stackvo.key`'de duruyor ve repository secret'ı olarak
   eklenmesi gerekiyor; Apple/Windows secret'ları ücretli hesaplara bağlı. #21
   bunun arkasında bekliyor.
4. **Kapsam eşiği.** Ölçüm var, kapı yok. %61.60'ı mı yoksa daha düşük bir
   tabanı mı kilitleyeceği mühendislik değil, politika kararı.

**Cevaplananlar:**

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
  REPL". §5.5 bunu bir görev değil bir **karar** olarak tutuyordu, çünkü yazılı
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

---

## 7. Ölçüm

Mekanik olarak sayılabilenler koda karşı tutuluyor:
`src-tauri/tests/platform_matrix_claims.rs` yanlış bir sayıda build'i kırıyor.

| | Sayı | Nasıl sayıldı |
|---|---|---|
| Toplam IPC komutu | **248** | `contracts/ipc.json` → `commands` (245 Rust + 3 `frontend-plugin`) |
| Bunlardan `#[tauri::command]` olarak yazılmış | **244** | `commands.rs`, `#[cfg(test)]` dışı |
| Frontend kaynak dosyası | **132** | `src/**/*.{js,vue}`, spec dosyaları hariç |
| Bunlardan `@tauri-apps` kullanan | **20** | aynı küme içinde metin taraması |
| **Veri katmanının geçtiği fonksiyon** | **1** (`src/lib/ipc.js` → `call()`) | `invoke(` `ipc.js` dışında **0** yerde geçiyor |
| `ipc.js` sarmalayıcısı | **241** | `api` nesnesinin üye sayısı |
| Rust kaynağı | **94 modül, 81.839 satır** | `src-tauri/src/*.rs` |

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
