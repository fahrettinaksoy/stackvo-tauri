# StackVo — servis paketleri ve market mimarisi

**Ölçüm tarihi: 11 Ağustos 2026.** Hedef: servisleri uygulamanın içinden
çıkarıp `stackvo/stackvo-service-packages` deposundan indirilen paketlere
dönüştürmek, ve aynı servisin birden çok sürümünü yan yana çalıştırılabilir
kılmak.

Bu doküman bir **tasarım ve süreç raporu**. Kod değiştirmiyor; neyin
değişmesi gerektiğini, hangi sırayla, hangi çıkış kriteriyle ve hangi riski
kabul ederek değişmesi gerektiğini söylüyor. `docs/durum.md` bu deponun
durumunu tutar; bu dosya onun **C-1, C-2 ve D-2** satırlarının nasıl
kapanacağını anlatır.

**Beş açık karar kapandı.** `docs/durum.md` §6'da **ADR 0011–0015** olarak
kayıtlı; §11 cevapları ve bu rapora yansımalarını topluyor. Karar
gerektirmediği için Faz 0 tamamlanmış sayılır ve Faz 1 planlanabilir.

## Nasıl ölçüldü

Aşağıdaki her sayı bugün ağaca karşı çalıştırılmış bir komutun çıktısı,
hatırlanmış değil. Komutlar §1'in sonunda duruyor, çünkü bir sonraki
okuyucunun aynı ölçümü tekrar edebilmesi bu raporun tek doğrulanabilir kısmı.

`✅` var · `🟡` yarım · `⬜` yok · `🔒` karar bekliyor

---

## 1. Bugün ne var

### 1.1 Bir servis tek bir yerde durmuyor

Bir servisin StackVo'da var olması için **dokuz** ayrı yerde adının geçmesi
gerekiyor. Bu, `docs/durum.md` §1'in D-1 maddesinde zaten kayıtlı ve bu
raporun çıkış noktası:

| # | Yer | Ne tutuyor | Tür |
| --- | --- | --- | --- |
| 1 | `skeleton/core/templates/services/<id>/` | compose şablonu + servis konfig şablonu | dosya, binary'de gömülü |
| 2 | `src-tauri/src/template.rs` → `DYNAMIC_SERVICES` | 25 girişlik **sıralı** dizi | derleme zamanı sabiti |
| 3 | `src-tauri/src/config.rs` → `EMBEDDED` | 185 girişlik varsayılan tablosu | derleme zamanı sabiti |
| 4 | `contracts/env.schema.json` → `services` | kategori → id listesi | sözleşme |
| 5 | `src-tauri/src/commands.rs` → `RENDERED` | hangi konfig dosyası hangi şablondan | derleme zamanı sabiti |
| 6 | `src-tauri/src/connect.rs` → `Spec` tablosu | bağlantı dizesi üretimi | derleme zamanı sabiti |
| 7 | `src-tauri/src/migrate.rs` | yabancı compose'dan servis tanıma | derleme zamanı sabiti |
| 8 | `src/i18n/` | görünen ad, açıklama | sözlük |
| 9 | `src-tauri/tests/fixtures/golden/` | dondurulmuş çıktı | test verisi |

Dokuzunun **altısı derleme zamanı sabiti**. Bu tek cümle, "market" fikrinin
neden bir özellik değil bir mimari değişiklik olduğunu açıklıyor: bugün bir
servis eklemek **yeni bir binary** gerektiriyor. Marketin tanımı ise tam
tersi — yeni binary olmadan yeni servis.

### 1.2 Servis durumu `.env`'de, ve `.env` düz

`.env` sözleşmesi (`contracts/env.schema.json` → `parsing`) bilerek naif: ilk
`=` kazanır, tırnak yok, interpolasyon yok, iç içe geçme yok. Bir servisin
tüm durumu `SERVICE_<ID>_*` anahtar ailesinde:

```
SERVICE_MYSQL_ENABLE=true
SERVICE_MYSQL_VERSION=8.0
SERVICE_MYSQL_ROOT_PASSWORD=…
```

`SERVICE_MYSQL_VERSION` **tek bir değer** taşır. `mysql@8.0` ve `mysql@9.4`
aynı anda açık olduğunda bu anahtarın ne olacağının cevabı yok — ve
`.env`'in düz olması bilinçli bir karar (Bash yükleyici ve Node ayrıştırıcı
aynı dosyayı okuyor), yani "iç içe geçirelim" bir çözüm değil.

**Sonuç: çoklu örnek `.env` üstünde çözülemez.** Yeni bir durum deposu
gerekiyor.

### 1.3 Her isim tekil

Ölçüm — 25 şablon dizininde:

| Ne | Kaç | Örnek | Çoklu örnekte ne olur |
| --- | :-: | --- | --- |
| `container_name:` | 26 | `stackvo-mysql` | ikinci örnek başlamaz: "name is already in use" |
| adlandırılmış volume | 18 | `stackvo-mysql-data` | iki sürüm **aynı veri dizinini** paylaşır → 8.0 datadir'ini 9.4 upgrade eder, geri dönüşü yok |
| compose servis anahtarı | 25 | `services: mysql:` | ikinci blok birincinin üstüne merge olur, sessizce |
| compose profili | 25 | `profiles: ["services","mysql"]` | `--profile mysql` iki örneği ayıramaz |
| Traefik alt alan adı | 12 | `SERVICE_PHPMYADMIN_URL` | iki örnek aynı domain'i ister |

26. `container_name` Kafka'nın kendi Zookeeper'ı — bir paketin birden çok
container taşıyabildiğinin kanıtı, ve paket formatının bunu desteklemesi
gerektiğinin.

En tehlikelisi ikinci satır. `stackvo-mysql-data` volume'ünü iki sürüm
paylaştığında Docker hata vermez; MySQL 9.4 açılışta 8.0'ın datadir'ini
görür ve yükseltir. Bu geri alınamaz ve kullanıcı ne olduğunu ancak 8.0'ı
tekrar açtığında anlar. **Çoklu örnek desteği, volume adı örnek başına
ayrılmadan açılırsa veri kaybı üretir.** Bu, bu raporun tek "yapmadan önce
şunu yap" maddesi.

### 1.4 Port tahsisi elle, ve şimdiden tutarsız

Şablonlarda **iki ayrı port anahtarı ailesi** var:

| Aile | Kaç anahtar | `config.rs`'in `EMBEDDED`'ında var mı |
| --- | :-: | --- |
| `HOST_PORT_<ID>` | 17 | **hayır, hiçbiri** |
| `SERVICE_<ID>_HOST_PORT` | 14 | 14'ü de var |

Bunun iki ölçülebilir sonucu var:

- **MySQL'in host portu arayüzden değiştirilemiyor.** Şablon
  `{{ HOST_PORT_MYSQL | default('3306') }}` okuyor; `HOST_PORT_MYSQL`
  hiçbir yerde varsayılan olarak tanımlı değil, dolayısıyla ayarlar sayfası
  o satırı hiç göstermiyor. Aynısı mariadb, mongo, redis, memcached,
  cassandra, elasticsearch, grafana, kibana, mailhog, mailpit, rabbitmq ve
  blackfire için de geçerli — **13 servis**.
- **Bir çakışma şimdiden yazılı.** `mongo-express` şablonunun varsayılanı
  `8081`, `phpmyadmin` şablonunun varsayılanı da `8081`. İkisi aynı anda
  açıldığında ikincisi bağlanamaz. Bugün kurtaran şey `config.rs`'in
  `SERVICE_MONGO_EXPRESS_HOST_PORT=8083` yazması — yani doğru cevap
  şablonda değil, başka bir dosyada, ve şablonun kendisi hâlâ yanlış.

Elle tahsis, tek örnekli bir dünyada bile bir çakışma üretmiş durumda.
109 sürümün her biri örnek olabildiğinde elle tahsis bir seçenek değil.

### 1.5 "Kapat" bugün "sil" demek

`commands.rs` → `service_disable`, sırayla: hosts girdisini kaldırır,
container'ı **durdurur ve siler**, sonra `discard_service` ile image'ı,
adlandırılmış volume'lerini ve log dizinini siler.

Yorumu bunu açıkça savunuyor ve tek örnekli dünyada savunması sağlam:
"kapalı" bir etiket değil bir durum olmalı. Ama market modelinde bu
**yanlış katmanda** duruyor: bir sürümü geçici olarak kapatmak, o sürümün
veritabanını silmek anlamına gelemez. Market modeli iki ayrı fiil gerektirir
ve bugün tek fiil var.

### 1.6 Katalog kapalı

`checked_service` → `env_schema().knows_service(name)`. `env.schema.json`'da
olmayan bir id reddediliyor, ve doğru sebeple: bilinmeyen bir id `.env`'e
`SERVICE_<ÇÖP>_ENABLE` yazar ve hiçbir şeye karşılık gelmeyen bir profil
açar.

Market ise **açık katalog** demek. Kapıyı kaldırmak değil, kapının
dayanağını değiştirmek gerekiyor: derlenmiş bir listeden, çalışma zamanında
doğrulanan bir paket manifestine.

### 1.7 Ölçüm komutları

```bash
# 1.1 — dokuz dokunma noktası
rg -l 'DYNAMIC_SERVICES|EMBEDDED|RENDERED' src-tauri/src/

# 1.3 — container adları ve volume'ler
rg -c 'container_name' skeleton/core/templates/services/*/*.tpl
rg -o 'stackvo-[a-z-]+-data' skeleton/core/templates/services/*/*.tpl | sort -u

# 1.4 — iki port ailesi
rg -o '\{\{ *(HOST_PORT_[A-Z_]+|SERVICE_[A-Z_]+_HOST_PORT)' \
   skeleton/core/templates/services/*/*.tpl | sort -u

# 1.7 — sürüm sayısı (109)
rg -o '\("SERVICE_[A-Z_]+_VERSIONS", "[^"]*"\)' src-tauri/src/config.rs \
  | awk -F', ' '{n=split($2,a,","); t+=n} END {print t}'
```

---

## 2. İstenen dört şeyin gerçek maliyeti

| İstek | Ne gerektiriyor | Zorluk |
| --- | --- | :-: |
| Servisler gömülü olmasın | `skeleton/`'dan 32 dosya çıkar, `DYNAMIC_SERVICES` ve `RENDERED` veri olur, `EMBEDDED`'ın servis yarısı taşınır | orta |
| Market'ten indir | Yeni: registry, indirici, doğrulayıcı, yerel paket deposu, IPC yüzeyi, UI | **büyük** |
| Aynı servisin çoklu sürümü | Yeni durum modeli (instance), ad/port/volume tahsisi, render hattının yeniden yazımı, `connect.rs` ve `migrate.rs`'in örnek-farkında olması | **en büyük** |
| İndirmeyi kaldır | Yeni fiil ayrımı: disable ≠ uninstall; veri sahipliği kararı | küçük, ama §1.5'i değiştirir |

İkisi arasındaki bağımlılık tek yönlü ve önemli: **çoklu örnek, marketten
bağımsız olarak gerekli ve önce gelmeli.** Market, "hangi paketler var"
sorusunu çözer; çoklu örnek, "bir paket kaç kere kurulabilir" sorusunu.
İkincisi çözülmeden birincisi kurulursa, market yalnızca bugünkü tekil
modelin uzaktan beslenen hâli olur ve asıl istenen şey (mysql 8.0 ile
9.4 yan yana) yine yapılamaz.

---

## 3. Hedef mimari

Üç ayrı katman, üç ayrı sorumluluk. Her biri diğerinden bağımsız test
edilebilir olmalı; bu, aşamalandırmayı mümkün kılan tek şey.

```
  ┌─────────────────────────────────────────────────────────────┐
  │  KAYNAK — stackvo/stackvo-service-packages                  │
  │  paket tanımı, sürüm başına. Kod yok, veri var.             │
  │  CI: şema doğrulama + compose doğrulama + tag probu         │
  └───────────────────────┬─────────────────────────────────────┘
                          │  HTTPS + imzalı registry.json
                          ▼
  ┌─────────────────────────────────────────────────────────────┐
  │  YEREL DEPO — ~/.stackvo/market/                            │
  │  indirilmiş, hash'i doğrulanmış, salt-okunur paketler       │
  │  market.rs: fetch → verify → unpack → kaydet                │
  └───────────────────────┬─────────────────────────────────────┘
                          │  paket manifesti
                          ▼
  ┌─────────────────────────────────────────────────────────────┐
  │  ÖRNEKLER — ~/.stackvo/services/instances.json              │
  │  hangi paket, kaç kere, hangi port, hangi volume, açık mı   │
  │  instances.rs: tek doğruluk kaynağı                         │
  └───────────────────────┬─────────────────────────────────────┘
                          │  render (ADR 0002: üretilir, düzenlenmez)
                          ▼
  ┌─────────────────────────────────────────────────────────────┐
  │  ÇIKTI — generated/docker-compose.dynamic.yml               │
  └─────────────────────────────────────────────────────────────┘
```

Bugünkü akışla farkı tek cümlede: **`.env` artık servislerin durumunu
tutmuyor; `instances.json` tutuyor. `.env` yalnızca yığın-biçimlendiren
tercihler için kalıyor** (domain, TLS, sunucu direktifleri) — zaten
`config.rs`'in ikinci grubunun tarif ettiği şey.

### 3.1 Paket deposunun dizin düzeni

İstenen `database > mysql > 8.0` hiyerarşisi, `env.schema.json`'ın zaten
kullandığı kategori adlarıyla:

```
stackvo-service-packages/
├── registry.json                     # üretilen indeks — elle yazılmaz
├── registry.json.minisig             # ed25519 imza
├── schema/
│   ├── package.schema.json           # servis düzeyi manifest şeması
│   ├── version.schema.json           # sürüm düzeyi manifest şeması
│   └── registry.schema.json
├── packages/
│   ├── databases/
│   │   ├── mysql/
│   │   │   ├── package.json          # servis kimliği: ad, ikon, kategori, bakımcı
│   │   │   ├── icon.svg
│   │   │   ├── README.md
│   │   │   └── versions/
│   │   │       ├── 9.7/
│   │   │       │   ├── manifest.json     # sürüm sözleşmesi
│   │   │       │   ├── compose.yml.tpl   # tek servis, tek fragment
│   │   │       │   └── files/
│   │   │       │       └── my.cnf.tpl
│   │   │       ├── 9.4/
│   │   │       ├── 8.4/
│   │   │       ├── 8.0/
│   │   │       └── 5.7/
│   │   ├── mariadb/    (6 sürüm)
│   │   ├── postgres/   (7 sürüm)
│   │   ├── mongo/      (6 sürüm)
│   │   └── cassandra/  (5 sürüm)
│   ├── cache/          redis(6) memcached(3) valkey(5)
│   ├── queue/          rabbitmq(5) kafka(4)
│   ├── search/         elasticsearch(5) kibana(5) meilisearch(4) typesense(4)
│   ├── storage/        minio(1)
│   ├── monitoring/     grafana(5)
│   ├── devtools/       mailhog(3) mailpit(4) blackfire(3)
│   └── admin-uis/      phpmyadmin(4) adminer(4) pgadmin(4) kafbat(4)
│                       mongo-express(4) phpcacheadmin(3)
└── tools/
    ├── build-registry.mjs            # packages/ → registry.json
    ├── validate.mjs                  # her manifest şemaya karşı
    └── probe-tags.mjs                # her image:tag gerçekten var mı
```

**25 servis, 109 sürüm dizini.** Sayılar bugünkü `SERVICE_<ID>_VERSIONS`
listelerinden geliyor; tam döküm §12'de.

#### Neden sürüm başına ayrı dizin, tek şablon + değişken değil

Bugün tek şablon var ve sürüm bir değişken (`mysql:{{ VERSION }}`). Bu,
sürümler arası fark olmadığı sürece çalışır — ve olmadığı doğru değil:

- MySQL 5.7 → 8.0: `caching_sha2_password` varsayılan oldu, eski istemciler
  için `--default-authentication-plugin` gerekiyor.
- Elasticsearch 7 → 8: güvenlik varsayılan olarak **açık**;
  `xpack.security.enabled=false` olmadan 8 hiç açılmıyor, 7'de o anahtar
  gereksiz.
- RabbitMQ: `management` etiketi bazı serilerde var bazılarında yok — bu
  yüzden `examples/service_tags.rs` etiketi şablondan okuyor.
- MongoDB 5 → 6 → 7: konfigürasyon dosyası anahtarları değişti.

Tek şablon bunları `{{ if }}` ile taşımaya kalkarsa şablon bir programa
dönüşür; ve bir programın paket deposundan indirilip çalıştırılması tam
olarak §4'ün engellemeye çalıştığı şey. **Sürüm başına düz, koşulsuz,
okunabilir bir dosya** hem güvenlik hem bakım açısından doğru taraf. Bedeli
tekrar; tekrarın bedeli `tools/validate.mjs`'in yakalayabileceği bir şey.

### 3.2 Sürüm manifesti — sözleşmenin kalbi

`packages/databases/mysql/versions/8.0/manifest.json`:

```json
{
  "apiVersion": "stackvo.dev/package/v1",
  "service": "mysql",
  "version": "8.0",
  "image": {
    "repository": "mysql",
    "tag": "8.0",
    "digest": "sha256:…",
    "registry": "docker.io"
  },
  "capabilities": ["database", "sql"],
  "instancing": {
    "multiple": true,
    "identity": "version"
  },
  "ports": [
    { "name": "sql", "container": 3306, "preferred": 3306, "protocol": "tcp", "primary": true }
  ],
  "volumes": [
    { "name": "data", "container": "/var/lib/mysql", "purgeable": true }
  ],
  "files": [
    {
      "template": "files/my.cnf.tpl",
      "target": "/etc/mysql/conf.d/stackvo.cnf",
      "mode": "0444",
      "sha256": "…"
    }
  ],
  "settings": [
    { "key": "ROOT_PASSWORD", "type": "secret", "default": "root", "required": true },
    { "key": "DATABASE",      "type": "string", "default": "stackvo" }
  ],
  "connection": {
    "scheme": "mysql",
    "user": "root",
    "passwordSetting": "ROOT_PASSWORD",
    "database": "DATABASE"
  },
  "health": {
    "test": ["CMD", "mysqladmin", "ping", "-h", "127.0.0.1"],
    "interval": "10s",
    "retries": 12
  },
  "dependsOn": [],
  "compose": { "file": "compose.yml.tpl", "sha256": "…" },
  "support": {
    "status": "supported",
    "eolDate": "2026-10-25",
    "source": "https://endoflife.date/api/mysql.json"
  },
  "notes": {
    "tr": "5.7'den yükseltirken authentication plugin değişikliğine dikkat.",
    "en": "Note the authentication plugin change when upgrading from 5.7."
  }
}
```

`version` alanı **somut** olmak zorunda. `latest` bir sürüm değil bir takma
ad ve bu formatta yeri yok: sabitlenmiş bir digest'i olmayan bir manifest,
§4.2'nin hash zincirine bağlanamaz. Takma ad registry düzeyinde
`recommended` ile ifade ediliyor (ADR 0014).

`support` bloğu bir görüş değil ölçüm: `tools/eol.mjs` onu endoflife.date'e
karşı doğruluyor ve sapma PR'ı kırıyor. Değerleri `supported`, `deprecated`,
`eol`.

Bu manifestte **beş şey** bugün kodda dağınık duran şeyleri tek yere
topluyor, ve her biri bir dosyanın küçülmesi demek:

| Manifest alanı | Bugün nerede | Ne olur |
| --- | --- | --- |
| `ports` | 31 şablon değişkeni, iki farklı isimlendirme | port tahsisi veri olur (§3.5) |
| `volumes` | şablon metninden regex ile çıkarılıyor (`discard_service`) | silme güvenli olur |
| `connection` | `connect.rs`'in 11 girişlik derlenmiş `Spec` tablosu | tablo boşalır, veri paketten gelir |
| `settings` | `config.rs`'in `EMBEDDED`'ının servis yarısı | ~90 satır `EMBEDDED`'dan çıkar |
| `dependsOn` | `env.schema.json` → `serviceDependencies` (9 giriş, eksik) | paket kendi bağımlılığını beyan eder |

`instancing.multiple: false` olan paketler de var — Traefik gibi tekil
altyapı, ya da `blackfire` gibi tek probe. Bunu manifest söylemeli, kod
tahmin etmemeli.

### 3.3 Compose fragmenti — ne olabilir, ne olamaz

`compose.yml.tpl`, bugünkü şablonların yaptığı işi yapar ama **tek servis
tanımlar** ve top-level `services:` / `volumes:` başlığı taşımaz. Bu,
`template.rs`'in `service_body` awk taklidini — 80 satırlık, "yirmi dosya
üzerinde yük taşıyan" yeniden girintileme mantığını — tamamen ortadan
kaldırır. Bugünkü şablonların `ports:` ve `networks:` bloklarını sütun
sıfırda yazması bir Bash devri kalıntısı ve yeni formatta taşınmamalı.

```yaml
# packages/databases/mysql/versions/8.0/compose.yml.tpl
image: "{{ image }}"
container_name: "{{ instance.container }}"
restart: unless-stopped
environment:
  MYSQL_ROOT_PASSWORD: "{{ settings.ROOT_PASSWORD }}"
  MYSQL_DATABASE: "{{ settings.DATABASE }}"
command: >
  mysqld
  --character-set-server=utf8mb4
  --collation-server=utf8mb4_unicode_ci
volumes:
  - "{{ volume.data }}:/var/lib/mysql"
  - "{{ file.my_cnf }}:/etc/mysql/conf.d/stackvo.cnf:ro"
ports:
  - "{{ port.sql }}:3306"
networks:
  {{ network }}:
    aliases: {{ instance.aliases }}
```

**Şablon rastgele `.env` okuyamaz.** Bugünkü `template.rs` şablona tüm
değişken haritasını veriyor; yeni modelde render bağlamı manifestin beyan
ettiklerinden ibaret: `image`, `instance`, `settings.*` (yalnız manifestte
tanımlı anahtarlar), `port.*`, `volume.*`, `file.*`, `network`. Bu bir kolaylık
kısıtı değil, §4'ün ilk savunma hattı — indirilen bir şablonun kullanıcının
tüm `.env`'ini bir `environment:` satırına yazıp dışarı sızdırması bu
kısıt olmadan mümkün.

### 3.4 Örnek (instance) modeli

Yeni tek doğruluk kaynağı: `~/.stackvo/services/instances.json`. Atomik
yazılır (`atomic.rs` zaten var), şema sürümlü, ve **kullanıcının elle
düzenlemesi beklenmiyor** — `.env`'in aksine, bu bir uygulama durumu.

```json
{
  "schemaVersion": 1,
  "instances": [
    {
      "id": "mysql-8-0",
      "service": "mysql",
      "version": "8.0",
      "package": { "source": "official", "sha256": "…", "installedAt": "2026-08-11T09:12:00Z" },
      "enabled": true,
      "primary": true,
      "aliases": ["stackvo-mysql-8-0", "stackvo-mysql"],
      "ports": { "sql": 3306 },
      "volumes": { "data": "stackvo-mysql-8-0-data" },
      "settings": { "DATABASE": "stackvo" },
      "secretRefs": { "ROOT_PASSWORD": "keychain:stackvo/mysql-8-0/ROOT_PASSWORD" }
    },
    {
      "id": "mysql-9-4",
      "service": "mysql",
      "version": "9.4",
      "enabled": true,
      "primary": false,
      "aliases": ["stackvo-mysql-9-4"],
      "ports": { "sql": 3316 },
      "volumes": { "data": "stackvo-mysql-9-4-data" },
      "settings": { "DATABASE": "stackvo" },
      "secretRefs": { "ROOT_PASSWORD": "keychain:stackvo/mysql-9-4/ROOT_PASSWORD" }
    }
  ]
}
```

#### Kimlik türetimi

`<service>@<version>` → slug: sürümdeki `.` ve `+` karakterleri `-` olur,
küçük harfe indirilir, DNS etiketine uygun hâle gelir.

| Kaynak | Slug | Container | Volume | Compose anahtarı | Profil |
| --- | --- | --- | --- | --- | --- |
| `mysql@8.0` | `mysql-8-0` | `stackvo-mysql-8-0` | `stackvo-mysql-8-0-data` | `mysql-8-0` | `mysql-8-0` |
| `mysql@9.4` | `mysql-9-4` | `stackvo-mysql-9-4` | `stackvo-mysql-9-4-data` | `mysql-9-4` | `mysql-9-4` |

Slug çakışması teorik olarak mümkün (`1.0.0` ve `1-0-0` aynı slug'a gider) ve
kurulum sırasında reddedilmeli — sessizce ikinci örneğin birinciyi ele
geçirmesinden iyisi bir hata mesajı.

#### Birincil örnek ve eski adın korunması

Projelerin `.env`'lerinde `DB_HOST=stackvo-mysql` yazıyor — bu, kullanıcının
kodunda, bu uygulamanın dokunamayacağı yerde. Yeni container adı
`stackvo-mysql-8-0` olursa **her mevcut proje kırılır**.

Çözüm: bir servisin **en fazla bir** örneği `primary` olabilir ve birincil
örnek eski adı ağ takma adı olarak taşır:

```yaml
networks:
  stackvo-net:
    aliases: ["stackvo-mysql-8-0", "stackvo-mysql"]
```

Docker'ın ağ takma adı tam da bunun için var ve container adıyla aynı
çözünürlüğü verir. Kullanıcı arayüzden "birincil yap" diyerek bunu başka bir
örneğe devredebilir; iki örneğin aynı takma adı istemesi render öncesi
reddedilmeli, çünkü Docker bunu **hata olarak değil rastgele olarak** çözer
— iki container aynı alias'a sahipse DNS ikisi arasında dönüşümlü cevap
verir, ve ortaya çıkan hata "bazen bağlanıyor" olur ki teşhisi en pahalı
sınıf.

### 3.5 Port tahsisi

Yeni modül: `ports.rs`. Dört girdi, tek çıktı.

1. **Manifestin tercihi.** `preferred: 3306`. Boşsa aynen verilir — tek
   örnekli kullanıcı bugünkü portunu görür, hiçbir şey değişmez.
2. **Rezervasyonlar.** `instances.json`'daki tüm portlar, örnek kapalı olsa
   bile. Kapalı bir örneğin portunu başkasına vermek, o örnek açıldığında
   çakışma demektir.
3. **Aralık.** Tercih doluysa servis başına deterministik bir aralıktan ilk
   boş: `preferred + 10·n`. `mysql@9.4` → 3316. Rastgele değil deterministik,
   çünkü kullanıcı portu ezberler ve bir yeniden kurulumda değişmesi
   sinir bozucu.
4. **Gerçek bağlanabilirlik.** Tahsis anında `SO_REUSEADDR` olmadan bir
   `bind` denemesi. `instances.json` StackVo dışındaki bir programın portu
   tuttuğunu bilemez; bunu yalnızca çekirdek bilir.

Tahsis edilen port `instances.json`'a **yazılır ve orada kalır** — her
render'da yeniden hesaplanmaz. Yeniden hesaplama, kullanıcının bağlantı
dizesinin bir güncelleme sonrası sessizce değişmesi demek.

`HOST_PORT_<ID>` ve `SERVICE_<ID>_HOST_PORT` aileleri bu modelde ortadan
kalkar. Göç sırasında `.env`'de bulunan değerler o örneğin tahsisi olarak
alınır (§7).

### 3.6 Render hattı

`generate` bugün `.env` okuyup 25 sabit girişi dolaşıyor. Yenisi
`instances.json` okuyup kurulu paketleri dolaşır:

```
instances.json ──┐
                 ├─→ ports.rs (doğrula, çakışma yok)
market/packages ─┤
                 ├─→ her açık örnek için:
                 │     manifest oku → render bağlamı kur → compose.yml.tpl render et
                 │     → compose policy validator (§4.4)
                 │     → files/*.tpl → generated/configs/<instance-id>/<file>
                 │
                 └─→ birleştir → generated/docker-compose.dynamic.yml
```

İki örnekli MySQL'in çıktısı:

```yaml
services:
  mysql-8-0:
    profiles: ["services", "mysql-8-0"]
    image: "mysql:8.0"
    container_name: "stackvo-mysql-8-0"
    ports: ["3306:3306"]
    volumes:
      - "stackvo-mysql-8-0-data:/var/lib/mysql"
      - "/Users/…/.stackvo/generated/configs/mysql-8-0/my.cnf:/etc/mysql/conf.d/stackvo.cnf:ro"
    networks:
      stackvo-net:
        aliases: ["stackvo-mysql-8-0", "stackvo-mysql"]
  mysql-9-4:
    profiles: ["services", "mysql-9-4"]
    image: "mysql:9.4"
    container_name: "stackvo-mysql-9-4"
    ports: ["3316:3306"]
    volumes:
      - "stackvo-mysql-9-4-data:/var/lib/mysql"
      - "/Users/…/.stackvo/generated/configs/mysql-9-4/my.cnf:/etc/mysql/conf.d/stackvo.cnf:ro"
    networks:
      stackvo-net:
        aliases: ["stackvo-mysql-9-4"]
volumes:
  stackvo-mysql-8-0-data:
  stackvo-mysql-9-4-data:
```

ADR 0002 korunuyor: bu dosya **her koşuda sıfırdan yazılır**, hiçbir zaman
düzenlenmez. Yeni olan tek şey girdisinin `.env` değil `instances.json`
olması.

### 3.7 Yerel disk düzeni

ADR 0011 gereği `market/` **ilk çekimden önce yoktur** — boş değil, yok. Bu
ayrım UI'a kadar taşınmalı: "katalog boş" ile "bu makinede henüz katalog
yok" farklı iki cümle ve ikincisi ne yapılacağını söyler.

```
~/.stackvo/
├── .env                      # yığın tercihleri — servis durumu ARTIK BURADA DEĞİL
├── market/                   # ilk `market_refresh`'e kadar hiç oluşmaz
│   ├── registry.json         # önbelleklenmiş indeks + ETag
│   ├── registry.json.minisig
│   ├── trust/
│   │   └── known_keys.json   # pinlenmiş anahtarlar + politika ile eklenenler
│   └── packages/
│       └── databases/mysql/8.0/    # indirilmiş, doğrulanmış, salt-okunur (0555)
│           ├── manifest.json
│           ├── compose.yml.tpl
│           ├── files/my.cnf.tpl
│           └── .stackvo-lock   # doğrulanan hash + kurulum zamanı
├── services/
│   └── instances.json        # ← tek doğruluk kaynağı
├── generated/                # ADR 0002 — her koşuda yeniden yazılır
│   ├── docker-compose.dynamic.yml
│   └── configs/<instance-id>/…
└── logs/services/<instance-id>/
```

`market/packages/` **salt-okunur** açılmalı (0555 dizin, 0444 dosya). Bu
performans için değil: kullanıcının oradaki bir şablonu düzenleyip
"neden hash doğrulaması başarısız" sorusunu sormasını engellemek için.
Düzenlemek isteyen `skeleton.rs`'in bugün yaptığı işi yapar — bir override
dizinine kopyalar (§10, Faz 7).

---

## 4. Güvenlik ve tedarik zinciri

Bu bölüm raporun **en kritik** kısmı, ve nedeni tek cümlede: bugün StackVo
yalnızca kendi binary'sinin içindeki dosyaları çalıştırıyor; bu değişiklikten
sonra **internetten indirdiği tanımları Docker'a verecek.** Docker'a verilen
bir compose fragmenti, doğru yazıldığında host dosya sistemine tam erişimdir.

### 4.1 Tehdit modeli

| # | Tehdit | Sonuç | Karşılık |
| --- | --- | --- | :-- |
| T-1 | Sahte registry (DNS/MITM) | rastgele paket kurulumu | HTTPS + imza (§4.2) |
| T-2 | Depo ele geçirilir | tüm kullanıcılara kötü paket | imza + iki kişi onayı + `known_keys` pinleme |
| T-3 | Kötü niyetli compose fragmenti | host'a root erişimi | **policy validator (§4.4)** |
| T-4 | Kötü niyetli image | container içi çalıştırma | digest pinleme + registry allowlist |
| T-5 | Şablon `.env` sızdırır | kimlik bilgileri dışarı | kısıtlı render bağlamı (§3.3) |
| T-6 | Downgrade / replay | eski, açığı olan paket | registry'de monoton `sequence` + `expires` |
| T-7 | Yol geçişi (`../`) paket içinde | keyfi dosya yazımı | açma sırasında yol normalleştirme + reddetme |
| T-8 | Zip bomb / dev paket | disk dolması | boyut ve dosya sayısı sınırı |

T-3 en ciddi olanı ve tek başına bir bölümü hak ediyor.

### 4.2 Güven zinciri

```
pinlenmiş ed25519 açık anahtarı (binary'de)
    └─ registry.json.minisig doğrular → registry.json
         └─ registry.json her paket sürümü için sha256(manifest.json) taşır
              └─ manifest.json her dosya için sha256 taşır
                   └─ compose.yml.tpl, files/*.tpl
```

Her adım bir öncekine bağlı. Ortada bir dosya değişirse zincir kopar ve
kurulum, dosya diske açılmadan **önce** reddedilir.

`registry.json`'ın imzası `minisign` formatında olmalı — Tauri'nin
güncelleyicisi zaten aynı aileyi kullanıyor, ve `docs/durum.md` §5'in 4.
maddesi (güncelleme endpoint'i ve imzalama secret'ları) **hâlâ karara
bağlanmamış**. Bu iki iş aynı anahtar yönetimi turunda çözülmeli: iki ayrı
imzalama altyapısı kuran bir projede biri mutlaka bakımsız kalır.

**Anahtar rotasyonu baştan tasarlanmalı.** `known_keys.json` birden çok
anahtar taşır; yeni anahtar eskisiyle imzalanmış bir `key-rotation.json` ile
tanıtılır. Rotasyon planı olmayan bir pinleme, anahtar sızdığında tek
çözümü "herkes uygulamayı güncellesin" olan bir pinlemedir.

### 4.3 Registry biçimi

```json
{
  "schemaVersion": 1,
  "sequence": 412,
  "generatedAt": "2026-08-11T09:00:00Z",
  "expires": "2026-09-11T09:00:00Z",
  "packages": [
    {
      "service": "mysql",
      "category": "databases",
      "name": { "tr": "MySQL", "en": "MySQL" },
      "instancing": { "multiple": true },
      "versions": [
        {
          "version": "9.7",
          "path": "packages/databases/mysql/versions/9.7",
          "manifestSha256": "…",
          "sizeBytes": 4211,
          "recommended": false,
          "support": "supported"
        },
        { "version": "8.0", "…": "…", "recommended": true,  "support": "supported" },
        { "version": "5.7", "…": "…", "recommended": false, "support": "eol" }
      ]
    }
  ]
}
```

`recommended` iki iş yapıyor ve ikincisi ADR 0014'ten geliyor: listenin
varsayılan seçimi olmak, **ve `latest`'in karşılığı olmak**. Bir istemci
`latest` istediğinde aldığı şey `recommended` sürümüdür ve o somut sürüm
`instances.json`'a yazılır — takma ad orada saklanmaz. `support` alanı
sıralamayı belirler: `eol` olanlar listede varsayılan olarak gizli.

`sequence` monoton artar ve istemci **daha küçük bir sequence'i reddeder** —
T-6'nın karşılığı. `expires` geçmişse istemci uyarır ama çalışmayı
durdurmaz; hava boşluklu (air-gapped) bir kurulumda bayat ama doğru bir
registry, hiç registry olmamasından iyidir.

### 4.4 Compose policy validator

Render edilmiş her fragment, birleştirilmeden önce bir doğrulayıcıdan geçer.
Bu, indirilen içeriğe karşı tek gerçek savunma ve **allowlist** olmalı,
blocklist değil — engellenecekler listesinin eksik kalması yalnızca zaman
meselesi.

**Kesin ret:**

| Kural | Neden |
| --- | --- |
| `privileged: true` | container kaçışı, tek satır |
| `network_mode: host` | ağ izolasyonunun tamamı |
| `pid: host`, `ipc: host`, `userns_mode: host` | aynı |
| `cap_add:` (izinli küçük küme dışında) | `SYS_ADMIN` = root |
| `security_opt: apparmor:unconfined` / `seccomp:unconfined` | çekirdek savunmasını kapatır |
| `devices:` | host aygıtı |
| `/var/run/docker.sock` bind | **Docker soketi = host'ta root** |
| workspace dışına bind mount | keyfi dosya okuma/yazma |
| `build:` (yerel Dockerfile) | keyfi derleme, keyfi indirme |
| `extends:` / `env_file:` | doğrulama kapsamının dışına çıkar |

**İzinli:** `image`, `container_name`, `restart`, `environment`, `command`,
`entrypoint`, `ports`, `expose`, `volumes` (yalnız adlandırılmış volume ve
workspace altındaki üretilmiş konfig yolları), `networks`, `depends_on`,
`healthcheck`, `labels`, `ulimits`, `deploy.resources`, `user`,
`working_dir`, `tmpfs`, `sysctls` (allowlist).

Doğrulayıcı **render sonrası** çalışmalı, öncesi değil. Şablonun
`{{ settings.X }}` içine ne koyduğu ancak render edildikten sonra
görülebilir, ve `X`'in kullanıcı tarafından girilebilir olması saldırının en
doğal yolu.

Bu doğrulayıcı bir Rust modülü (`compose_policy.rs`) ve testlerinin her
biri bir saldırı olmalı. `security-review` skill'inin bakacağı ilk dosya bu.

### 4.5 Sırlar

`secrets.rs` ve anahtarlık zaten var; ADR 0010 sırların `.env`'den çıkmasını
kayda geçirmiş. Örnek modelinde sır **örnek başına** olmalı:
`mysql-8-0` ve `mysql-9-4` farklı parolalar taşıyabilmeli. `instances.json`
sır değil `secretRef` tutar; `config::MASK` mekanizması olduğu gibi geçerli.

Paket manifesti bir sırrın **varsayılan değerini** verebilir (`"root"`), ama
o değer kurulumda anahtarlığa yazılır ve orada kullanıcının değiştirmesi
beklenir. Manifestin gerçek bir kimlik bilgisi taşıması CI'da reddedilmeli —
`contracts/CONFLICTS.md` C-18 (Blackfire kimlik bilgileri `.env.example`'da)
aynı hatanın bir önceki turu.

### 4.6 Üçüncü taraf paketler

Bu raporun önerisi: **v1'de yok.** Yalnızca resmî depo, pinlenmiş anahtar.

Üçüncü taraf, T-2 ve T-3'ü niteliksel olarak değiştirir ve bir moderasyon
süreci, bir yayıncı kimliği ve bir kaldırma (takedown) mekanizması gerektirir
— hiçbiri bu turda kurulabilecek şeyler değil. Mimarinin buna **hazır**
olması (kaynak alanı, imza doğrulayıcı, policy validator) yeterli; kapının
açılması ayrı bir karar.

`policy.rs` bu kapının kurumsal tarafını zaten tutabiliyor:
`market.allowedSources`, `market.registryUrl`, `market.requireSignature`,
`market.allowedRegistries`. Docker Hub'a erişemeyen bir ağdaki kurumsal
kullanıcı, politikada kendi aynasını (mirror) gösterir — `policy.rs`'in
başlık yorumunda anlatılan senaryonun tam olarak aynısı.

---

## 5. Sözleşme değişiklikleri

ADR 0006: IPC sözleşmesi yazılır, üretilmez. ADR 0008: kırıcı değişiklik
nedir. İkisi de bu bölümde geçerli.

### 5.1 `contracts/env.schema.json`

`services` bloğu ve `servicePattern` **kaldırılmıyor, dondurulmuyor da** —
anlamı daralıyor: artık "bu uygulamanın bildiği servisler" değil,
"bu uygulamanın göç edebileceği eski servis kimlikleri". `serviceDependencies`
paketlere taşınır (`manifest.dependsOn`).

`SERVICE_<ID>_*` anahtarları **eski (legacy)** işaretlenir. Okunmaya devam
eder — göç için — ama yazılmaz.

### 5.2 Yeni sözleşmeler

| Dosya | Ne tanımlar | Kim doğrular |
| --- | --- | --- |
| `contracts/package.schema.json` | sürüm manifesti | her iki repo'nun CI'ı |
| `contracts/registry.schema.json` | registry indeksi | her iki repo'nun CI'ı |
| `contracts/instances.schema.json` | yerel örnek durumu | stackvo testleri |

Üçü de `tools/validate-contracts.mjs`'e eklenir. Kritik nokta: **paket
şeması iki repo tarafından paylaşılır.** Kopyalanırsa kayar. Öneri: şema
`stackvo/contracts` altında yaşar ve paket deposu onu bir git submodule ya
da CI'da indirilen sürümlü bir dosya olarak alır — kopya değil, referans.

### 5.3 `contracts/ipc.json`

Yeni komutlar (13) ve olaylar (9). Hepsi `"new": true` ile eklenir; hiçbiri
mevcut bir komutun imzasını değiştirmez, dolayısıyla ADR 0008'e göre kırıcı
değil.

| Komut | Tür | Döner |
| --- | --- | --- |
| `market_status` | query | `MarketStatus` (registry yaşı, imza durumu, kaynak) |
| `market_refresh` | mutation | `operation_id` |
| `market_catalog` | query | `Vec<MarketPackage>` |
| `market_install` | mutation | `operation_id` |
| `market_uninstall` | mutation | `void` (+ `purgeData: bool`) |
| `instance_list` | query | `Vec<Instance>` |
| `instance_plan` | query | `InstancePlan` (kurulmadan önce: ayarlar + tahsis edilecek portlar) |
| `instance_create` | mutation | `InstanceId` (+ `settings`, `ports` — ikisi de opsiyonel) |
| `instance_remove` | mutation | `void` |
| `instance_enable` / `instance_disable` | mutation | `operation_id` |
| `instance_start` / `instance_stop` / `instance_restart` | mutation | `operation_id` |
| `instance_settings` / `instance_apply_settings` | query / mutation | — (`apply` ayrıca `ports` alır) |
| `instance_connection` | query | `Option<Connection>` |
| `instance_promote` | mutation | `void` (birincil takma adı devret) |

`instance_plan` hiçbir şey yazmaz ve hiçbir şey rezerve etmez; döndürdüğü
portlar **şu an** tahsis edilecek olanlardır ve `instance_create` gerçekte
yeniden tahsis eder — ikisi arasında başka bir şey o portu almış olabilir. Bir
`secret`'ın değeri burada **maskesiz** döner, ki bu uygulamadaki tek istisnadır
ve sebebi değerin ne olduğudur: ortada örnek de keystore kaydı da yoktur, bu
manifestin yayınlanmış ilk-boot varsayılanıdır ve zaten diskteki bir JSON
dosyasındadır.

Portlar `instance_apply_settings`'e kendi argümanı olarak gelir — `patch`'in
içine değil, çünkü ayar değiller; ve *kendi komutu* olarak değil, çünkü ikisi de
konteyneri durdurup yeniden kurmayı gerektirir ve iki komut bunu tek düğmeye
basış için iki kez yapardı.

Olaylar mevcut `service:*` ailesini yansıtır: `market:refreshing`,
`market:refreshed`, `package:installing`, `package:progress`,
`package:installed`, `package:removed`, `instance:enabling`,
`instance:enabled`, `instance:error`.

**Eski `service_*` komutları bir sürüm boyunca kalır**, birincil örneğe
yönlendiren ince sarmalayıcılar olarak. Kaldırılmaları ayrı bir kırıcı
değişiklik ve `contracts/surface.lock.json` bunu zaten yakalayacak.

### 5.4 İlerleme raporlaması

ADR 0005: uzun işlemler bir sink üzerinden rapor verir. Paket indirme
uzun bir işlem ve aynı yoldan geçmeli — `runner::run_operation`'ın yaptığı
şeyi bir HTTP indirmesi için yapan bir eş. Bu, `progress.rs`'in zaten
tanımladığı olay şeklini yeniden kullanmalı, ikinci bir ilerleme kavramı
icat etmemeli.

---

## 6. Kod tarafı: ne değişir

| Modül | Bugün | Sonra | Etki |
| --- | --- | --- | :-: |
| `template.rs` | `DYNAMIC_SERVICES` sabiti, `service_body` awk taklidi | render motoru kalır, katalog ve awk gider | **−200 satır** |
| `config.rs` | `EMBEDDED`'ın ~90 servis satırı | yığın tercihleri kalır | −90 satır |
| `connect.rs` | 11 girişlik derlenmiş `Spec` tablosu | manifest'ten okuyan tek fonksiyon | −150 satır |
| `commands.rs` | `service_*`, `RENDERED`, `discard_service` | `instance_*` + `market_*`, sarmalayıcılar | ≈ eşit |
| `skeleton.rs` | 32 servis şablonu gömülü | yalnız `core/compose`, `core/servers` | −32 dosya |
| `env_writer.rs` | `set_service_enabled` | eski; `instances.rs` yazar | küçülür |
| `migrate.rs` | derlenmiş servis tanıma | katalogdan tanıma | küçülür |

**Yeni modüller:**

| Modül | Sorumluluk | Saf mı |
| --- | --- | :-: |
| `market.rs` | registry çekme, önbellek, ETag | hayır (ağ) |
| `pkg.rs` | manifest ayrıştırma, doğrulama, hash zinciri | **evet** |
| `trust.rs` | imza doğrulama, anahtar pinleme, rotasyon | **evet** |
| `instances.rs` | örnek tablosu okuma/yazma, kimlik türetimi | **evet** |
| `ports.rs` | port tahsisi ve çakışma tespiti | yarı (bind denemesi) |
| `compose_policy.rs` | render sonrası allowlist doğrulaması | **evet** |

Altı modülün dördü saf. Bu tesadüf değil, hedef: ADR 0001'in "domain bandı
Tauri'yi bilmez" kuralı burada da geçerli, ve saf modüller ağsız test
edilebiliyor — güvenlik testlerinin ağa bağlı olması onları çalıştırılmaz
kılar.

Net satır etkisi kabaca **sıfır** (silinen ≈ eklenen), ama silinen kısım
derleme zamanı sabiti, eklenen kısım çalışma zamanı mantığı. Bu takas
raporun tamamının konusu: **esneklik, doğrulama maliyetiyle satın alınır.**

---

## 7. Geriye uyumluluk ve göç

Bugün çalışan bir kurulum var (`~/.stackvo/.env` yedi servisi açık
gösteriyor) ve projelerin `.env`'lerinde `stackvo-mysql` yazıyor. Göç bir
kerelik, otomatik ve **geri alınabilir** olmalı.

### Göç algoritması

1. `instances.json` yoksa ve `.env`'de `SERVICE_*_ENABLE` varsa göç tetiklenir.
2. `.env`'i tara. Her `SERVICE_<ID>_ENABLE=true` için:
   - Sürüm: `SERVICE_<ID>_VERSION`, yoksa paketin `recommended` sürümü.
     **`latest` ise somutlaştır** (ADR 0014) — registry'nin o anki
     `recommended` sürümü `instances.json`'a yazılır, `latest` yazılmaz.
     Bugünkü 25 varsayılanın 11'i bu yoldan geçiyor. Yan etkisi istenen
     yönde: `latest` yazan bir kurulum bugün bir image yeniden çekildiğinde
     sessizce sürüm atlayabiliyor; somutlaştırma bunu bitiriyor ve yükseltme
     bir kullanıcı eylemi hâline geliyor. Sürüm notunda yazılmalı.
   - Sürüm depoda yoksa göç **durur ve raporlar** — sessizce en yakınına
     düşmez. ADR 0014'ün "yayımlanmış sürüm silinemez" kuralı bu durumun
     yalnızca elle bir hatayla oluşabilmesini sağlıyor.
   - Örnek oluştur, `primary: true`, eski takma adı ver.
   - Port: `.env`'de `HOST_PORT_<ID>` ya da `SERVICE_<ID>_HOST_PORT` varsa
     onu **rezerve et**; yoksa manifest tercihini.
   - **Volume'ü yeniden adlandırma.** Bu örnek `stackvo-mysql-data`'yı
     kullanmaya devam eder (`volumes.data` alanına yazılır). Yeni örnekler
     yeni adı alır. Var olan veriye dokunmayan tek yol bu.
   - Ayarlar: `SERVICE_<ID>_*` anahtarları örneğin `settings`'ine;
     sırlar anahtarlığa.
3. `.env`'in servis satırları **silinmez**, `# migrated to instances.json`
   yorumuyla işaretlenir. Geri dönüş, `instances.json`'ı silmek.
4. Bir yedek: `.env` → `.env.pre-market.bak`.
5. Göçün ilk `generate`'i, üretilen compose'u **eski dosyayla karşılaştırır**
   ve fark yalnızca beklenen alanlardaysa devam eder. Beklenmeyen bir fark
   göçü durdurur ve kullanıcıya raporlar. Bu, göçün tek gerçek güvenliği.

### Göç edilemeyecek olan

`SERVICE_PHPMYADMIN_HOST=stackvo-mysql` gibi **başka bir servise işaret
eden** ayarlar. Çoklu örnekte bunlar bir örnek referansı olmalı
(`"mysqlInstance": "mysql-8-0"`), ve göç birincil örneğe bağlar. Manifest
bunu bir ayar türü olarak tanımalı:

```json
{ "key": "HOST", "type": "instanceRef", "capability": "sql", "default": "primary" }
```

Bu, `env.schema.json`'ın `serviceDependencies` bloğunun eksik olduğunu kabul
eden notunun (C-16: "3 giriş / 20") gerçek çözümü: bağımlılık, bir paketin
kendi beyanı olur.

---

## 8. Test stratejisi

Bu değişiklik **iki test stratejisini birden** kırıyor ve ikisinin de
yerine ne konacağı önceden yazılmazsa, kapı olmadan ilerlenir.

### 8.1 Kırılan: golden fixture'lar

`src-tauri/tests/golden_render.rs`, 25 servisin birleştirilmiş çıktısını
bayt bayt donduruyor. Yeni modelde bu çıktı `instances.json` + kurulu
paketlerin fonksiyonu; sabit bir dosya değil.

**Yerine:** depoda pinlenmiş bir **test registry'si**
(`src-tauri/tests/fixtures/registry/`) — resmî deponun bir anlık görüntüsü,
sürümlü. Golden'lar bu registry'ye karşı, adlandırılmış örnek senaryolarıyla
üretilir:

| Senaryo | Ne dondurur |
| --- | --- |
| `single-mysql` | tek örnek, birincil, eski takma ad |
| `dual-mysql` | 8.0 + 9.4, port tahsisi, ayrı volume |
| `full-stack` | bugünkü 25 servisin örnek karşılığı — göç eşdeğerliği |
| `empty` | hiç örnek yok → `services: {}` (bugünkü davranış korunur) |

`full-stack` senaryosu özellikle değerli: çıktısı bugünkü golden'la
**alan alan** karşılaştırılabilir olmalı (adlar hariç), ve bu göçün doğru
olduğunun tek makine-kontrollü kanıtı.

### 8.2 Yeni: paket deposunun CI'ı

Paket deposu kendi başına test edilmeli, tüketicisini beklemeden:

| Kapı | Ne yapar | Neden |
| --- | --- | --- |
| `validate.mjs` | her `manifest.json` şemaya karşı | şema ihlali kurulumda değil PR'da yakalanır |
| `compose-config` | her fragment `docker compose config`'ten geçer | Compose spec'i doğrulayan tek referans |
| `policy` | her fragment `compose_policy` allowlist'inden geçer | **kötü paket depoya girmez** |
| `probe-tags.mjs` | her `image:tag` registry'de var mı | `examples/service_tags.rs`'in yaptığı iş, kaynağa taşınmış |
| `no-secrets` | manifestlerde gerçek kimlik bilgisi yok | C-18'in tekrarı |
| `digest-freshness` | pinlenmiş digest hâlâ çözülüyor mu | sessiz çürüme |
| `registry-build` | `registry.json` yeniden üretilir ve commit'le eşleşir | elle düzenlenmiş indeks olamaz |

`policy` kapısının **her iki tarafta** çalışması bilinçli: kaynakta bir
gözden geçirme, istemcide bir savunma. Yalnız kaynakta olursa ele geçirilmiş
depo her şeyi yollayabilir; yalnız istemcide olursa kötü paket kullanıcıya
kadar gelir.

### 8.3 Yeni: tüketici sözleşme testi

StackVo tarafında, pinlenmiş test registry'sine karşı: kurulum → doğrulama →
render → policy → `docker compose config`. Ağsız, deterministik, her PR'da.

Ayrıca **ağa dokunan ama CI'yı bloklamayan** bir kontrol —
`examples/service_tags.rs`'in bugünkü rolü: canlı registry'nin pinlenmiş
kopyadan ne kadar saptığını raporlar, kırmaz.

### 8.4 Güvenlik testleri

`compose_policy.rs`'in testleri, her biri bir saldırı olacak şekilde:
docker.sock bind'i, `privileged`, `network_mode: host`, workspace dışı bind,
`../` yol geçişi, `build:` context'i, `cap_add: SYS_ADMIN`,
`env_file`, kullanıcı girdisiyle enjekte edilen YAML. Her biri **reddedilmeli**
ve testin adı ne reddettiğini söylemeli.

`trust.rs` için: bozuk imza, yanlış anahtar, geri alınmış anahtar, düşük
`sequence` (replay), süresi geçmiş registry, zincirde kopuk hash.

---

## 9. Kurumsal katman

`policy.rs` zaten var ve yorumu bu senaryoyu neredeyse birebir anlatıyor:
"Docker Hub'ın erişilebilir olmadığı bir ağda image'ların çekildiği registry".
Market bu katmanı doğal olarak uzatır.

| Politika anahtarı | Ne yapar |
| --- | --- |
| `market.registryUrl` | kurumsal ayna |
| `market.additionalKeys` | kurumun kendi imzalama anahtarı |
| `market.requireSignature` | imzasız paket kesin ret (varsayılan: açık) |
| `market.allowedPackages` | yalnız onaylı servisler kurulabilir |
| `market.allowedRegistries` | image yalnız kurumsal registry'den |
| `market.autoUpdate` | paket güncelleme yasak/serbest |
| `market.offlineBundle` | yerel dosya yolundan kurulum |

ADR 0009 hâlâ geçerli ve tekrarlanmalı: **bu bir kilit değil.** Politika
işbirlikçi bir uygulamaya organizasyonun niyetini söyler; makineyi elinde
tutan kişiye karşı savunma değildir. Ama `requireSignature` bundan
farklı — o bir kilit, çünkü doğrulama uygulamanın kendi kodunda ve
politika yalnızca onu **gevşetemez**, sıkabilir. Bu asimetri açıkça
yazılmalı: politika bir güvenlik kontrolünü kapatabiliyorsa, o kontrol
güvenlik kontrolü değildir.

**Hava boşluklu kurulum**, ADR 0011'den sonra isteğe bağlı bir kurumsal
ekstra değil — **tek yol**. Binary hiçbir servis tanımı taşımadığı için ağı
olmayan bir makinede uygulamayı kullanılabilir kılan başka bir mekanizma
yok. Bu yüzden Faz 5'ten **Faz 4'e** taşındı.

`stackvo-packages.tar` — imzalı, tam registry + tüm paketler.
`market.offlineBundle` onu gösterir ve ağ hiç denenmez. Aynı doğrulama
zinciri geçerli: paket bir tar'dan geldi diye imza atlanmaz, çünkü tar
kopyalanabilir ve kopyalanan şey değiştirilebilir.

Kurumsal satın alma süreçlerinin sorduğu ilk soru bu, ve artık cevabı
"destekliyoruz" değil "mimarinin bir parçası".

---

## 10. Aşamalı yol haritası

Her fazın **ölçülebilir bir çıkış kriteri** var ve fazlar bağımsız olarak
sevk edilebilir. Sıra, `docs/durum.md` §4'ün "etki ÷ efor" kuralına değil,
**risk sırasına** göre: en tehlikeli değişiklik (veri modeli) en erken ve
görünür değişiklik olmadan yapılır.

### Faz 0 — Kararlar · **tamamlandı**

Beş karar kapandı ve `docs/durum.md` §6'ya **ADR 0011–0015** olarak yazıldı.
§11 cevapları ve bu rapora yansımalarını taşıyor.

### Faz 1 — Paket formatı ve kaynak depo · **tamamlandı**

`stackvo-service-packages` şemalarını, `tools/`'unu ve CI'ını kurar.
Sürümler mevcut şablonlardan **üretilerek** yazılır (elle değil — bugünkü
`skeleton/` bir dönüştürücünün girdisi olur, böylece dönüşüm tekrarlanabilir
ve gözden geçirilebilir).

**Kapsam, ADR 0014'ten sonra.** 109'un tamamı yayımlanır ama hepsi eşit
değil:

| Adım | Ne | Kaç |
| --- | --- | :-: |
| 1 | Bugünkü 25 varsayılan — göç tabanı, bunlar olmadan Faz 2 kırılır | 25 |
| 2 | Bakım gören seriler — `support: supported`, `recommended` adayları | ölçülür |
| 3 | Kalanlar — `support: eol`, listede gizli, ama **var** | fark |

Üçüncü adım isteğe bağlı değil: yayımlanmamış bir sürüm, o sürümü `.env`'inde
taşıyan bir kullanıcının göçünü durdurur. Yayımlamak ile öne çıkarmak
farklı iki şey ve ayrımı `support` alanı taşıyor.

`latest` **hiçbir adımda bir dizin değil**. Bugünkü 11 `latest` varsayılanı,
o servisin `recommended` sürümüne çözülür ve dönüştürücü bunu bir kez,
gözden geçirilebilir bir eşleme tablosuyla yapar.

**Çıkış — karşılandı.** 25 servis, 101 sürüm; her manifest şemaya uyuyor ve
istemcinin kendi ayrıştırıcısından (`pkg::parse`) geçiyor; 105 fragment
`docker compose config`'ten ve politika allowlist'inden geçiyor; hiçbir dizin
adı `latest` değil; `registry.json` üretiliyor ve `--check` ile ağaca karşı
tutuluyor. İmzalama **yapılmadı** — özel anahtar bir yayın adımına ait, ADR 0015.

**Yolda bulunanlar.** `latest` 11 varsayılanda; MinIO'nun tek sürümü de `latest`
olduğu için katalogda hiç somut sürümü yoktu (dördü ölçülüp eklendi).
`postgres.conf` ve `elasticsearch.yml` her generate'te yazılıp hiçbir şablon
tarafından mount edilmiyordu — paketlerde düzeltildi ve çalışan konteynerlerde
doğrulandı. `template.rs`'in her ASCII olmayan karakteri çift kodlaması; golden
fixture bunu yakalayamazdı çünkü karşılaştırmanın iki tarafı da bozuktu.
`eol.mjs` ilk çalıştığında 101 sürümün **20'sinin** `supported` dediği hâlde
end-of-life olduğunu ölçtü — üçü uygulamanın kendi önerdiği sürüm.

### Faz 2 — Örnek modeli ve göç · **tamamlandı**

`instances.rs`, `ports.rs`, göç. Render hattı `instances.json`'dan besleniyor
ama kaynak hâlâ **gömülü** şablonlar. Kullanıcı hiçbir fark görmüyor.

**En riskli faz, ve bu yüzden en görünmez olmalı.**

**Çıkış — karşılandı.** `instances.rs`, `ports.rs`, `pkg.rs`, `handover.rs`,
`render.rs`; hepsi saf, hiçbiri çalışan davranışa bağlı değil.
`tests/handover_equivalence.rs` göçü alan alan tutuyor: aynı image, aynı
yayınlanan port (elle değiştirilmiş `HOST_PORT_MYSQL=3399` dahil), aynı volume,
aynı environment, ve `stackvo-mysql` hâlâ çözülüyor. Kalan: `.env` yedeği ve
göçün arayüzü — Faz 3'ün UI turuyla birlikte.

**Yolda bulunan.** Dönüştürücü konfig şablonlarını birebir kopyalıyordu:
`redis.conf.tpl` `{{ REDIS_PASSWORD }}`, `elasticsearch.yml.tpl`
`{{ ELASTIC_SECURITY }}` okuyordu ve ikisi de hiçbir manifestte tanımlı değildi.
İkisi de `template::PREFIXES`'e uymadığı için eski renderer onları
`${REDIS_PASSWORD}` olarak bırakıyordu — ve bunlar compose değil **konfig**
dosyası, yani kimse interpolate etmiyordu. Paketlenmeleri, ikisinin de ilk kez
gerçek bir ayar olması demek.

### Faz 3 — Çoklu örnek

Kimlik türetimi, takma ad devri, port tahsisi, UI. Kaynak hâlâ gömülü.

**Çıkış — yarısı karşılandı.** Kimlik türetimi, takma ad devri ve port
tahsisi Faz 2'de indi; bu turda arayüz geldi: dokuz IPC komutu (`market_*`,
`instance_*`), `useMarket` composable'ı ve **Market sayfası** — katalog,
kurulum, örnek ekleme, birincil devri ve kaldırma.

**Çıkışın kalan yarısı da indi.** Faz 6'nın takasından sonra Docker'a dokunan
beş komut anlam kazandı ve yazıldı: `instance_enable`, `instance_disable`,
`instance_start`, `instance_stop`, `instance_restart`. Market sayfasının örnek
tablosunda artık bir aç/kapat anahtarı var.

**`instance_disable` hiçbir şey silmiyor** (ADR 0012). `service_disable`
container'ı, image'ı ve adlandırılmış volume'leri siliyor ve tek sürümlü bir
dünyada bu doğruydu — "kapalı" bir etiket değil bir durum olmalı. Sürüm başına
doğru olmaktan çıkıyor: MySQL 8.0'ı 9.4'ü denemek için kapatan biri geri
döndüğünde 8.0'ın satırlarını istiyor, ve datadir'i alıp götüren bir kapatma
bunu öğrenmenin en pahalı yolu olurdu. Silme `instance_remove` ve
`market_uninstall`'ın arkasında, düğmenin üstündeki kelimenin olan bitene
uyduğu yerde.

**Manuel doğrulama yapıldı ve programa dönüştü.**
`examples/side_by_side.rs` iki paketi kurup iki örnek yaratıyor, compose'u
render edip kaldırıyor, ikisine de bağlanıyor ve sonra hepsini siliyor. Bu
makinede: **8.0.46 ve 9.4.0**, ayrı volume'ler, ayrı portlar (3316 ve 3326 —
3306'yı makinenin kendi MySQL'i tutuyordu, yani tahsis de ölçüldü), ve
`stackvo-mysql` ağ içinde çözülüyor.

**Yolda iki gerçek hata bulundu, ikisi de yalnız çalıştırınca görünür.**

`docker compose up --wait` ikisini de "healthy" bildirdi ve ikisi de bağlantıyı
reddetti: **hiçbir paket healthcheck bildirmiyor**, ve bildirilmediğinde
`--wait` yalnız "süreç başladı" demek. MySQL ilk açılışta datadir'i portu
açmadan önce kuruyor. Manifest formatının `health` alanı tam bunun için var ve
bugün her pakette boş — çünkü üretildikleri şablonlarda yoktu. Katalogdaki
gerçek bir boşluk: onsuz `depends_on: condition: service_healthy` ağaçtaki
hiçbir servis için bir şey ifade etmiyor.

Ve **MySQL 9.x hiç açılmıyordu** — `contracts/CONFLICTS.md` C-21. Sürüm seçicisi
9.7 ve 9.4 sunuyor, tek bir `my.cnf` her sürüme mount ediliyor, ve iki
direktifi MySQL 9 kaldırmış. 9.4/9.7 paketleri sürüm başına düzeltildi; bu, bir
sürüm bir dizin kuralının var olma sebebinin ta kendisi.

Bu fazın sonunda **kullanıcının asıl istediği şey çalışıyor** — market
olmadan. Bu, aşamalandırmanın amacı.

### Faz 4 — Market, yerel kaynakla · *+ hava boşluğu*

`market.rs` ve `trust.rs`. Kaynak: `file://` ile pinlenmiş test registry'si.
Ağ yok, UI var.

**İkisi öne alındı ve bitti.** `pkg.rs` Faz 2'de gerekti — göç, örnek tablosunu
manifestler olmadan dolduramıyordu. `compose_policy.rs` ise render hattıyla
birlikte yazıldı, çünkü politikanın render *sonrası* çalışması gerekiyor ve onu
sonraya bırakmak, uygulamanın bir tur boyunca yalnız yayın zamanı incelenmiş
fragment'leri çalıştırması demekti.

ADR 0011 bu faza bir madde ekliyor: **`market.offlineBundle` burada
tamamlanır**, Faz 5'te değil. Binary hiçbir servis tanımı taşımadığı için
hava boşluklu kurulumun tek yolu bu, ve `file://` kaynağı zaten bu fazın
konusu — ikisi aynı kod yolu.

**Çıkış — kısmen karşılandı.** `market.rs` yazıldı: `Source` trait'i ve yerel
kaynak, indeks çekme, sequence geri gitme reddi, hash zincirinin orta halkası
(indeks → manifest, baytlar önce), atomik kurulum ve kaldırma. Politika ve
render tarafındaki saldırı testleri reddediyor.

**İmza doğrulaması yazılmadı, ve sebebi bir engel.** ADR 0015 registry'ye kendi
ed25519 anahtarını veriyor; onu üretecek tören `docs/durum.md` §5'in 4.
maddesinde hâlâ **açık bir karar**. Buraya bir yer tutucu anahtar yazmak
boşluktan kötü olurdu — sonraki her okuyucu zincirin kapandığına inanırdı. O
yüzden `Trust::Signed` istendiğinde `refresh` **reddediyor**: sessizce
imzasıza düşen bir güvenlik kontrolü, hiç olmayandan kötüdür, çünkü olmayan
görünür.

Yerel kaynakla çalışan bir makine bundan etkilenmiyor; ağ kaynağına bakan bir
makine etkilenecek — ve doğru sıra bu, çünkü ağ kaynağı Faz 5.

#### Arayüz yarısı, ayrı bir turda · **tamamlandı**

"UI var" bu fazın çıkışıydı ve kurma/kaldırma/aç/kapat için doğruydu. Model
bittikten sonra sorulan ikinci soru — **sınırdan ne geçiyor, ekranda ne
görünüyor** — arayı büyük buldu, ve aradaki her şey zaten hesaplanıp atılıyordu:

- **Sağlık.** Bu depo 24 healthcheck'i gerçek konteynerde yeşile getirmişti ve
  hiçbir ekran cevabı okumuyordu. `engine::health_from_status` liste satırından
  okuyor — yirmi satırlık bir sayfayı çizmek için yirmi `inspect` etmemek için —
  ve durum çipi artık `running` ile `healthy`'yi ayırıyor. Healthcheck'i olmayan
  konteyner eski iki kelimede kalıyor; onun için üçüncü bir kelime uydurmak aynı
  fazla-iddianın tersi olurdu.
- **`container_inspect` on dokuz alan dönüyordu, panel dördünü okuyordu.** Çıkış
  kodu (137 = bellek), restart sayısı ve politikası, uptime, çalışan imaj ve
  boyutu. Bellek için öldürülen bir servis "Durdu" deyip susuyordu.
- **Companion'lar.** Dört Kafka manifestinin Zookeeper'ı compose'a derleniyor ve
  `commands.rs`'te kelime hiç geçmiyordu: satır yok, durum yok, loguna erişim
  yok. Konteyner adı `render::context` ile aynı yerden türetiliyor — elle senkron
  tutulan ikinci bir türetme, yanlış konteynerin sağlığını raporlayan satırdır.
- **`support`/`eolDate` yalnız katalog ağacındaydı**, yani kurduğunuz sayfada;
  end-of-life bir veritabanında hata arayan kişi orada durmuyor.
- **Port handle'ları.** `Object.values(...)` isimleri atıyordu: MinIO "9000,
  9001" olarak okunuyor ve hangisinin konsol olduğunu tablo söyleyemiyordu.
- **`keywords` sınırı hiç geçmiyordu** (registry v1'den beri yayınlıyor), yani
  yirmi beş servis ve yüz sürüm sekiz kapalı kategorinin arkasında aranamıyordu.

İki gerçek yetenek de bu turda indi ve ikisi de §1'in teşhislerinin kalanıydı:
**kurulum formu** (§3.2'nin `settings` bloğu ilk boot'ta okunur; formsuz uygulama
`root` ile kurup sonra düzenlemeyi öneriyordu, ki bu bir veritabanında işe
yaramaz) ve **port değiştirme** (§1.4'ün eleştirisi elle tahsisti; tahsis
otomatikleşti ama *değiştirilemez* hâle geldi — `HOST_PORT_MYSQL` en azından
düzenlenebilir bir satırdı).

### Faz 5 — Market, ağ kaynağıyla · **tamamlandı**

`market::HttpSource`. `market_refresh` ve `market_install` artık bir URL de
kabul ediyor ve hangisi olduğu dizeden anlaşılıyor — kullanıcıya az önce
yazdığı şeyi bir radyo düğmesiyle tekrar ettirmek, sorulmayacak bir soru.

**Çıkış — karşılandı, ve dört kararla.**

**`http://` reddediliyor**, sunucuya bırakılmıyor. §4.2'nin zinciri henüz
var olmayan bir imzada başlıyor (ADR 0015), yani bugün taşıma katmanı
korumanın *tamamı*; `http://` onu da kaldırırdı.

**Sistem proxy'si kullanılıyor, ve bu `mail.rs`'in tam tersi.** O istemci
`no_proxy()` ile kuruluyor çünkü yalnız 127.0.0.1'e konuşuyor ve kurumsal bir
proxy'nin orada işi yok. Burada tersi doğru: kurumsal ayna zaten proxy'nin
arkasında, ve `market.registryUrl`'in var olma sebebi o makine.

**ETag, ikinci yenilemeyi ucuz ve dürüst yapıyor.** `304` bir hata da boş bir
cevap da değil — önbellekteki kopyanın güncel olduğu anlamına geliyor ve
çağıran onu tutuyor. Doğrulayıcı indeksin yanında değil, ayrı tutuluyor: biri
bir aktarım hakkında, öteki bir katalog hakkında.

**Gövde sınırı alınan bayta göre, `Content-Length`'e göre değil** (T-8). O
başlığı gönderen yazıyor; sayılan tek şey gerçekten gelen olabilir.

Bir de çağıranlara düşen bir kısıt: `Source::fetch` senkron — `pkg` ve
`render` bu trait'i okuyor ve ikisinin de bir runtime'ın ne olduğunu bilmesi
gerekmiyor — o yüzden `HttpSource` mevcut handle üzerinde blokluyor, ve bunu
bir runtime iş parçacığında yapmak panikler. `market_refresh` ile
`market_install` bütün işi `spawn_blocking` içinde koşturuyor.

### Faz 6 — Gömülü şablonların kaldırılması · **tamamlandı**

`skeleton/core/templates/services/` silinir. `DYNAMIC_SERVICES`, `RENDERED`,
`connect.rs`'in `Spec` tablosu, `EMBEDDED`'ın servis yarısı silinir.

**Takas indi.** `render_generated` servis yarısını iki kaynaktan üretiyor ve
kural bilerek keskin: `instances.json` **yoksa** eski yol, bayt bayt
değişmeden — bugün var olan her çalışma alanı bu durumda ve hiçbiri bu sürümü
fark etmiyor. **Varsa** yeni yol; biri bilerek göç etmiştir ve o andan sonra
tablo, bu çalışma alanının hangi servisleri çalıştırdığı hakkındaki gerçektir.

**Geri düşme yok.** Var olup render edilemeyen bir tablo, içinde bir ad geçen
bir hatadır; sessizce `.env`'den render etmek, kullanıcının çoktan değiştirdiği
bir durumdan yığın kurmak olurdu — tam da tablonun bitirdiği kayma, "güvenlik"
yazan kapıdan içeri girerek. `tests/handover_equivalence.rs` → `the_switch`
dört yönü de tutuyor.

**Ön koşul indi: ağ kapısı yazıldı.** `CatalogueGate.vue`, `RequirementsGate`
ile `BootstrapGate` arasında — bu sıra bir tercih değil: bootstrap compose
dosyalarını yazıp yığını kaldırıyor, ve katalogsuz bir makinede yazacak servis
olmadığı için arkasında hiçbir şey olmayan bir proxy'yle açılırdı. Kapı iki
durumu ayrı cümlelerle söylüyor — "internet yok" ve "bu makinede henüz katalog
yok" — çünkü yalnız ikincisinin cevabı hava boşluklu paket. **Atlanabilir**, ve
bu bir taviz değil: katalogsuz StackVo hâlâ bir ters vekil, bir sertifika
otoritesi ve bir proje koşturucusu, ve geçilemeyen bir ilk açılış ekranı
insanların uygulamayı kapattığı ekrandır.

**O karar verildi: ADR 0016, zorunlu göç, bir kapının arkasında.**
`MigrationGate`, `RequirementsGate`/`CatalogueGate`/`BootstrapGate` deseninin
dördüncüsü — katalogtan sonra, bootstrap'tan önce. Planı yazmadan önce
gösteriyor, `.env`'i yedekliyor, ve atlanabiliyor; öteki tarafta servissiz bir
uygulama var, eski yığın değil. `.env` dalı ve 25 şablon silindi; göç etmemiş
bir çalışma alanı `render_generated`'dan adıyla bir hata alıyor.

Kararın asıl gerekçesi iki yolun **iki farklı katalog** bilmesiydi: Solr ve
ClickHouse paket olarak gelince `services: ["solr"]` yazan bir proje doğru bir
beyana yanlış bir uyarı almaya başladı, ve o uyarı gömülü liste durdukça
düzeltilemiyordu. `docs/durum.md` §1'de S-16 kaydı, §6'da ADR 0016.

**Çıkış** — dördünden üçü indi: `skeleton/` altında hiçbir servis şablonu
kalmıyor (25 dizin, 128 KB silindi; `skeleton/` beş dosya);
`rg 'SERVICE_[A-Z_]+_(ENABLE|VERSION)' src-tauri/src/` yalnız göç kodunda ve
`config::EMBEDDED`'da isabet veriyor — o tablo göçün okuduğu şey ve
`docs/durum.md` §3'te 36. madde olarak duruyor;
binary boyutu ölçülüp raporlanıyor (ADR 0011'in ölçülebilir tek getirisi);
temiz kurulum uçtan uca çalışıyor; ağsız temiz kurulum kapıyı gösteriyor ve
çökmüyor.

### Faz 7 — Uzatma noktaları

Kullanıcının kendi paketi (yerel dizin), workspace override (`skeleton.rs`'in
`materialize`/`revert` deseninin paketlere uygulanması), üçüncü taraf kaynak
politikası. `docs/durum.md` C-1 ve C-2 burada kapanır.

---

## 11. Verilen kararlar

Beşi de kapandı. Gerekçeleriyle birlikte `docs/durum.md` §6'da **ADR
0011–0015** olarak duruyor; burada karar ve bu raporun hangi bölümlerinin
ona göre değiştiği var.

### K-1 → ADR 0011 · Hiçbir şey gömülü kalmaz ✅

Seçenek **A**. Ne şablon, ne konfig, ne de gömülü bir registry anlık
görüntüsü. Ağsız ilk açılışta katalog boş ve uygulama "ağ gerekli" der.

Ara çözüm (imzalı bir `registry.json`'ı gömmek) elenirken kullanılan gerekçe
kayda değer: gömülü her bayt bir sonraki sürüme kadar bayatlar, ve "gömülü
olan yalnızca liste" ayrımı altı ay sonra kimsenin hatırlamayacağı bir
ayrımdır. **Tek kural olarak "servis tanımı binary'de yoktur" savunulabilir;
"neredeyse yoktur" savunulamaz.**

Bu rapora yansıması:

- **§3.7** — `market/` dizini ilk çekimden önce **yok**, boş değil. Yokluk ile
  boşluk farklı iki durum ve UI ikisini farklı göstermeli.
- **§9** — `market.offlineBundle` artık isteğe bağlı bir kurumsal ekstra değil,
  **hava boşluklu kurulumun tek yolu**. Faz 5'e değil Faz 4'e taşınıyor.
- **§10, Faz 6** — ön koşulu artık belirsiz değil: bir **ağ kapısı** yazılmalı.
  `RequirementsGate` ve `BootstrapGate` deseninin üçüncüsü, ve aynı yerde
  yaşamalı.
- **§8** — pinlenmiş test registry'si "iyi olur" değil **zorunlu**: CI ve
  paketleme testleri ağa bağlanamaz.

Bir kez çekilmiş registry önbellekte kalıyor; engellenen yalnızca **hiç
çekmemiş** bir makine. Bu ayrım hata mesajında da görünmeli — "internet yok"
ile "bu makinede henüz katalog yok" farklı iki cümle.

### K-2 → ADR 0012 · Silen fiil `market_uninstall` ✅

| Fiil | Container | Image | Volume | Paket |
| --- | --- | --- | --- | --- |
| `instance_disable` | dur + sil | kalır | **kalır** | kalır |
| `instance_remove` | dur + sil | kalır | sorar | kalır |
| `market_uninstall` | dur + sil | siler | sorar (`purgeData`) | siler |

Bugünkü `service_disable` üç fiile bölünüyor ve yıkıcı olan en dıştakine
taşınıyor. `discard_service`'in mantığı korunuyor ama volume listesini
şablon metninden regex ile değil, manifestin `volumes[].purgeable` alanından
alıyor — §3.2'nin tablosunda zaten öyleydi, şimdi zorunlu.

İki ayrıntı sürüm notuna girmeli: bu bir **davranış değişikliği** (bugünkü
"kapat"ı temizlik olarak kullanan biri artık disk dolduracak), ve **kapalı
bir örneğin portu rezerve kalır** (§3.5'in ikinci girdisi).

### K-3 → ADR 0013 · Statik HTTPS ✅

İmzalı `registry.json` + düz dosyalar. OCI reddedilmedi, **ertelendi**;
kaynak bir `PackageSource` trait'inin arkasında duruyor ki ikinci taşıma
biçimi bir yeniden yazım değil bir uygulama olsun.

Yeni crate yok — `reqwest` zaten bağımlılık. Kurumsal ayna
`market.registryUrl` ile bir dosya sunucusuna işaret ediyor; Docker Hub oran
sınırları paket indirmeyi değil yalnız image çekmeyi etkiliyor, ki o zaten
bugünkü durum.

### K-4 → ADR 0014 · Desteklenen sürümler; `latest` bir dizin değil ✅

Karar iki parçalı ve ikincisi ölçümden çıktı.

**Birincisi:** depo 109 sürümle başlamıyor. Yayımlanan küme iki kümenin
birleşimi:

| Küme | Nedir | Neden zorunlu |
| --- | --- | --- |
| **A — göç tabanı** | bugün bir kullanıcının `.env`'inde yazılı olabilecek her sürüm | bu sürüm depoda yoksa Faz 2 göçü o kullanıcıda kırılır |
| **B — bakım gören** | upstream'de hâlâ destek alan seriler | marketin var olma sebebi |

A kümesinin **alt sınırı** bugünkü 25 varsayılan; üst sınırı bugünkü 109
sürümün kullanıcı `.env`'lerinde gerçekten geçenleri, ki bu ölçülemez —
dolayısıyla A pratikte "109'un tamamı, ama `support.status` ile işaretli"
olarak yayımlanır. Fark, **öncelik**: EOL bir sürüm listede öne çıkmaz,
`recommended` olamaz, ve UI onu "eski sürümleri göster" arkasına koyar.

"Destekli" bir görüş olamaz, ölçüm olmalı. Manifest bir `support` bloğu
kazanıyor ve `tools/eol.mjs` onu endoflife.date'e karşı doğruluyor; sapma
PR'ı kırıyor:

```json
"support": {
  "status": "supported",
  "eolDate": "2026-10-25",
  "source": "https://endoflife.date/api/mysql.json"
}
```

**Bir kez yayımlanmış sürüm registry'den silinemez** — yalnız işaretlenebilir.
Silinirse o sürümü kurmuş bir `instances.json` ortada kalır ve kullanıcının
çalışan bir servisi, kaynağı olmayan bir örneğe dönüşür.

**İkincisi — ölçümden çıkan:** bugünkü 25 varsayılanın **11'i `latest`**
(adminer, cassandra, grafana, kafbat, mailhog, mailpit, minio, mongo-express,
pgadmin, phpcacheadmin, phpmyadmin). `latest` bir paket sürümü **olamaz**:
sabitlenmiş bir digest'i, dolayısıyla §4.2'nin hash zincirinde bir yeri
yoktur. Registry düzeyinde bir takma ad oluyor — zaten var olan `recommended`
alanı — ve göç (§7) `SERVICE_<ID>_VERSION=latest`'i o anki somut sürüme
çözüp `instances.json`'a **somut olarak** yazıyor.

Bunun yan etkisi istenen yönde: bugün `latest` yazan bir kurulum, bir image
yeniden çekildiğinde sessizce sürüm atlayabiliyor. Somutlaştırma bunu
bitiriyor ve yükseltme bir kullanıcı eylemi hâline geliyor.

Sürüm başına ayrı dizin kararı (seri soyutlaması yerine) korunuyor: seri,
§3.1'de sayılan gerçek sürümler-arası farkları geri getirir ve şablonu
programa dönüştürür. Tekrar, koşuldan ucuzdur.

### K-5 → ADR 0015 · Ayrı anahtar ✅

İçerik imzası, Tauri güncelleyicisinin binary imzasından ayrı bir ed25519
anahtar çifti. `docs/durum.md` §5'in 4. maddesiyle aynı turda çözülüyor ama
aynı anahtarla değil.

Kazanç asimetride: güncelleyici anahtarı sızarsa sahte binary, içerik
anahtarı sızarsa sahte paket — **ikisi birden değil.** Bedeli iki sızma
yüzeyi, ve bunu ödemeye değer kılan tek şart prosedürün ortak olması: aynı
saklama yeri, aynı erişim listesi, aynı rotasyon adımları. İki ayrı prosedür,
bakımsız kalan bir prosedür demektir.

Rotasyon baştan tasarlanıyor (§4.2): `known_keys.json` birden çok anahtar
taşır, yeni anahtar eskisiyle imzalanmış bir kayıtla tanıtılır.

---

## 12. Ek — 25 servisin paket karşılığı

Kaynak: `config.rs` → `EMBEDDED` (`SERVICE_<ID>_VERSIONS`, `_VERSION`) ve
`env.schema.json` → `services`. Bugün sunulan **109 sürüm**, ve bunların
**11'inin varsayılanı `latest`** — ADR 0014'ten sonra dizin adı olamayacak
olan 11 giriş, aşağıda **kalın**.

Tablo bugünkü kaynağı gösteriyor, hedefi değil. Dönüştürücünün iki işi:
`latest` varsayılanlarını somut sürüme çözmek, ve her sürüme bir `support`
durumu yazmak (ölçümü `tools/eol.mjs` yapıyor, bu tablo değil).

| Kategori | Servis | Sürümler | Varsayılan | Şablon dosyaları |
| --- | --- | :-: | --- | :-: |
| databases | mysql | 5 — 9.7, 9.4, 8.4, 8.0, 5.7 | 8.0 | compose + my.cnf |
| databases | mariadb | 6 — 12.3, 11.8, 11.4, 10.11, 10.6, 10.5 | 10.6 | compose + my.cnf |
| databases | postgres | 7 — 18, 17, 16, 15, 14, 13, 12 | 14 | compose + postgres.conf |
| databases | mongo | 6 — 8.0, 8.3, 8.2, 7.0, 6.0, 5.0 | 8.0 | compose + mongo.conf |
| databases | cassandra | 5 — latest, 5.0, 4.1, 4.0, 3.11 | **latest** | compose |
| cache | redis | 6 — 8.10, 8.2, 7.4, 7.2, 7.0, 6.2 | 7.0 | compose + redis.conf |
| cache | memcached | 3 — 1.6, 1.5, 1.4 | 1.6 | compose |
| cache | valkey | 5 — 9.1, 9.0, 8.1, 8, 7.2 | 8 | compose + valkey.conf |
| queue | rabbitmq | 5 — 4.3, 4.2, 4, 3.13, 3 | 3 | compose |
| queue | kafka | 4 — 8.3.1, 7.9.9, 7.5.0, 6.2.15 | 7.5.0 | compose (2 container) |
| search | elasticsearch | 5 — 9.4.4, 9.3.8, 8.19.19, 8.11.3, 7.17.28 | 8.11.3 | compose + elasticsearch.yml |
| search | kibana | 5 — 9.4.4, 9.3.8, 8.19.19, 8.11.3, 7.17.28 | 8.11.3 | compose |
| search | meilisearch | 4 — latest, v1.53, v1.52, v1.11 | v1.11 | compose |
| search | typesense | 4 — 30.2, 29.1, 28.0, 27.1 | 27.1 | compose |
| storage | minio | 1 — latest | **latest** | compose |
| monitoring | grafana | 5 — latest, 13.1, 12.4, 11.6, 10.4.19 | **latest** | compose |
| devtools | mailhog | 3 — latest, v1.0.1, v1.0.0 | **latest** | compose |
| devtools | mailpit | 4 — latest, v1.30, v1.29, v1.28 | **latest** | compose |
| devtools | blackfire | 3 — 2, 2026.8.0, 2.30.3 | 2 | compose |
| admin-uis | phpmyadmin | 4 — latest, 5.2, 5.1, 5.0 | **latest** | compose |
| admin-uis | adminer | 4 — latest, 5.5.1, 5.4.2, 4.8.1 | **latest** | compose |
| admin-uis | pgadmin | 4 — latest, 9.17, 9.16, 8.14 | **latest** | compose |
| admin-uis | kafbat | 4 — latest, v1.5.0, v1.4.2, v1.3.0 | **latest** | compose |
| admin-uis | mongo-express | 4 — latest, 1.0.2, 1.0, 0.54 | **latest** | compose |
| admin-uis | phpcacheadmin | 3 — latest, 2.6.0, 2.5.2 | **latest** | compose |

**Paket üretilirken düzeltilecek dört şey** (bugün kaynakta yanlış):

1. `mongo-express` şablonunun port varsayılanı `8081` — `phpmyadmin` ile
   çakışıyor. Doğrusu `8083` ve yalnızca `config.rs`'te var.
2. 13 servisin host portu şablonda `HOST_PORT_<ID>` okuyor ama o anahtarın
   hiçbir varsayılanı yok; manifestin `ports.preferred` alanı bunu tek
   yerde çözer.
3. `postgres` varsayılanı `14`, sunulan en yeni sürüm `18`. Varsayılanın
   neden en yeni olmadığı manifestte `recommended` ile açıkça
   söylenmeli — ya da düzeltilmeli.
4. **11 servisin varsayılanı `latest`.** Bu, bugün de bir kusur: bir image
   yeniden çekildiğinde kullanıcının servisi sessizce sürüm atlayabiliyor ve
   bunu kimse istemiyor. ADR 0014 bunu paket formatının bir kısıtı olarak
   kapatıyor — `latest` bir dizin olamıyor — ve göç kurulumları
   somutlaştırıyor.

Dördü de `tools/validate.mjs`'in yakalayacağı sınıftan: port çakışması,
tanımsız anahtar, `recommended` ile varsayılanın ayrışması, `latest` adlı
dizin. Bu bir tesadüf değil — kaynakta bu dört hatanın oluşabilmiş olmasının
sebebi, kontrolün yapılabileceği tek bir yerin olmaması.

`instancing.multiple`: admin arayüzleri (`phpmyadmin`, `adminer`, `pgadmin`,
`kafbat`, `mongo-express`, `phpcacheadmin`) ve `blackfire` için `false`
başlangıç önerisi — bunlar tek bir alt alan adına bağlı ve çoklu örnek
domain çakışması üretir. Çoklu örnek desteği bunlara ancak alt alan adı da
örnek başına türetildiğinde (`phpmyadmin-5-2.stackvo.loc`) açılabilir; bu
ayrı ve daha küçük bir iş.

---

## 13. Reddedilen alternatifler

| Alternatif | Neden hayır |
| --- | --- |
| Paket deposunu git submodule yapmak | Kullanıcının makinesinde git gerektirir; `git.rs` bilinçli olarak yalnız kullanıcı deposu klonluyor; kısmi indirme yok — 109 sürümün tamamı iner |
| Tüm depoyu `git clone` etmek | Aynı; ayrıca güncelleme = tüm ağaç, ve imza doğrulaması git nesnelerine değil dosyalara yapılmalı |
| npm paketi olarak dağıtmak | Node bağımlılığı ekler; npm'in kendi tedarik zinciri riski bu projenin riskine eklenir |
| Tek dev `services.json` | 109 sürüm tek dosyada = her güncelleme tüm dosyayı indirir; kısmi doğrulama yok; birleştirme (merge) çatışmaları |
| Şablonu programlanabilir yapmak (Lua/Rhai) | İndirilen kod çalıştırmak. §4'ün tamamı bunu engellemek için var |
| Compose fragmentini doğrulamadan geçirmek | Docker soketi bind'i tek satır ve sonucu host'ta root |
| Örnek durumunu `.env`'de tutmak | `.env` düz ve öyle kalmalı; iki tüketicisi daha var |
| Portları her render'da yeniden hesaplamak | Kullanıcının bağlantı dizesi bir güncelleme sonrası sessizce değişir |
| Binary'ye imzalı bir registry anlık görüntüsü gömmek | ADR 0011. Gömülü her bayt bir sonraki sürüme kadar bayatlar; "gömülü olan yalnızca liste" ayrımı altı ay sonra hatırlanmaz |
| `latest`'i bir sürüm dizini yapmak | ADR 0014. Sabitlenmiş digest'i yok, dolayısıyla §4.2'nin hash zincirinde yeri yok |
| Yayımlanmış bir sürümü registry'den silmek | ADR 0014. Kurmuş bir kullanıcının çalışan servisi, kaynağı olmayan bir örneğe döner |
| EOL sürümleri hiç yayımlamamak | Aynı; göç, kullanıcının bugün `.env`'inde ne yazdığına bakmak zorunda ve o ölçülemez |

---

## 14. Bu dosya nasıl doğru kalır

`docs/durum.md` §8'in kuralı burada da geçerli: bu dosyadaki her sayı bir
komuttan geliyor ve komutlar §1.7'de yazılı.

Faz 1 tamamlandığında bu dokümanın §3 ve §5'i **sözleşme dosyalarına**
taşınmalı (`contracts/package.schema.json`'ın kendi açıklamaları), ve burada
yalnızca gerekçe kalmalı. Bir tasarım dokümanı, tarif ettiği şey var olduktan
sonra ikinci bir doğruluk kaynağıdır ve kayar.

Kararlar zaten taşındı: ADR 0011–0015 `docs/durum.md` §6'da ve gerekçenin
kalıcı adresi orası. §11 onların bu rapora yansımasını tutuyor, kendisi
kaynak değil — ikisi çelişirse §6 haklıdır.

Faz 6 tamamlandığında §1'in tamamı geçmiş zamana geçer ve bu dosyanın
anlatacağı bir şey kalmaz; o zaman silinir.
