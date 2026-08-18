export default {
  $vuetify: {
    badge: 'rozet',
    close: 'Kapat',
    dataIterator: { noResultsText: 'Sonuç bulunamadı', loadingText: 'Yükleniyor…' },
    noDataText: 'Veri yok',
  },

  app: {
    projects: 'Projeler',
    services: 'Servisler',
    settings: 'Ayarlar',
    refresh: 'Yenile',
    loading: 'Yükleniyor…',
    never: '—',
    cancel: 'Vazgeç',
    close: 'Kapat',
    copy: 'Kopyala',
    documentation: 'Belgeler',
    buyMeCoffee: 'Bir kahve ısmarla',
    socialMedia: 'Sosyal medya',
    language: 'Dil',
    toggleTheme: 'Temayı değiştir',
  },

  close: {
    title: 'StackVo kapatılsın mı?',
    subtitle:
      'Konteynerler Docker tarafından yönetiliyor; uygulama kapansa da çalışmaya devam edebilirler.',
    tray: 'Tepsiye küçült',
    trayHint: 'Uygulama arka planda kalır, yığın çalışmaya devam eder.',
    quit: 'Kapat, yığın çalışsın',
    quitHint: 'Uygulamadan çıkılır; konteynerlere dokunulmaz.',
    stopAndQuit: 'Her şeyi durdur ve kapat',
    stopAndQuitHint: 'Tüm StackVo konteynerleri durdurulur, sonra çıkılır.',
    remember: 'Bir daha sorma',
    behaviour: 'Kapatma davranışı',
    behaviourHint: 'Kapat düğmesine bastığında ne olacağını seç.',
    ask: 'Her seferinde sor',
  },
  nav: {
    dashboard: 'Panel',
    projects: 'Projeler',
    market: 'Katalog',
    logs: 'Loglar',
    dumps: 'Dump’lar',
    mail: 'Mail',
    settings: 'Ayarlar',
    collapse: 'Daralt',
    expand: 'Genişlet',
  },

  system: {
    docker: 'Docker',
    running: 'Çalışıyor',
    stopped: 'Durdu',
    containers: 'Konteynerler',
  },

  /**
   * Tepsi simgesi ve native menü çubuğu — ikisini de Rust çiziyor.
   *
   * Yalnızca başka yerde karşılığı olmayan dizeler burada: tepsinin dört
   * gezinme girdisi `nav`'dan, motor sözcükleri `system`'den, menü çubuğunun üç
   * bağlantısı `about.links`'ten geliyor.
   */
  tray: {
    checking: 'Docker denetleniyor…',
    show: "StackVo'yu aç",
    quit: 'Çık',
    engineDown: 'Docker çalışmıyor',
    engineUp: 'Docker çalışıyor',
    noWorkspace: 'StackVo dizini seçilmedi',
    noProjects: 'Proje yok',
    containers: 'Konteynerler: {count}',
    more: '+{count} proje daha…',
    runningSummary: '{running}/{total} proje çalışıyor',
    menuAbout: 'StackVo Hakkında',
    control: 'Başlat / durdur',
    startProject: '{name}: başlat',
    stopProject: '{name}: durdur',
    started: '{name} çalışıyor.',
    stopped: '{name} durdu.',
    failed: '{name} değiştirilemedi.',
  },

  /**
   * Komut paleti (A-2).
   *
   * `keys` kısayolu tuş resmiyle değil cümleyle veriyor: bu satır bir altbilgi
   * ve paleti ilk kez gören okuyucunun ihtiyacı olan şey cümle.
   */
  palette: {
    title: 'Komut paleti',
    placeholder: 'Bir komut ya da proje adı yazın…',
    empty: '“{query}” ile eşleşen bir şey yok.',
    keys: '↑ ↓ gezinir · Enter çalıştırır · Esc kapatır',
    sections: {
      navigate: 'Git',
      projects: 'Projeler',
      stack: 'Yığın',
      app: 'Uygulama',
    },
    /**
     * Proje fiilleri `actions.*`'ı kullanmıyor: oradakiler "Konteyneri başlat"
     * diyor, ki yazıldıkları düğme için doğru ama bir proje adının yanında
     * yanlış okunuyor — ve satır aynı zamanda okuyucunun yazdığı yer, yani en
     * kesin cümleyi değil en kısa doğru cümleyi istiyor.
     */
    project: {
      start: '{name} başlat',
      stop: '{name} durdur',
      restart: '{name} yeniden başlat',
      build: '{name} derle',
      site: '{domain} adresini tarayıcıda aç',
    },
  },

  /**
   * `stackvo.local.json` — bu makineye özel geçersiz kılmalar (B-2).
   *
   * Üç git durumundan yalnız `notIgnored` bir uyarı, ve neyin yanlış olduğunu
   * değil ne yapılacağını söylüyor: commit'e giren bir dosya artık makine
   * ayarı olmaktan çıkıp herkesin ayarı olur.
   */
  local: {
    title: 'Yalnız bu makine',
    explain:
      'Buradaki değerler bu checkout için stackvo.json’u geçersiz kılar ve commit’lenmek için değildir. Test ettiğiniz bir sürüm ya da bu makinede başka bir şeyle çakışan bir alan adı için.',
    applied: 'Bu makinede yürürlükte:',
    refused:
      'Yok sayıldı: {keys}. Bunlar bu makineyi değil depoyu tarif ediyor, o yüzden yalnız stackvo.json’dan okunuyor.',
    ignored: 'git bu dosyayı commit’lerin dışında tutuyor.',
    notIgnored:
      'git bu dosyayı commit’ler. stackvo.local.json’u .gitignore’a ekleyin — yoksa bu ayarlar tüm ekibin ayarı olur.',
    remove: 'Kaldır',
  },

  /**
   * Yaşam döngüsü hook'ları (B-3).
   *
   * `explain` yalnız özelliği değil riski adlandırıyor. Onaylamayı okumaktan
   * kolaylaştıran bir ekran, bu ekranın var olma nedeninin tersi olurdu.
   */
  hooks: {
    title: 'Bu proje başlarken ve dururken',
    explain:
      'stackvo.json\u2019da tanımlı komutlar. Konteynerde çalışan adımlar onay gerektirmez — konteyner zaten bu deponun kodunu çalıştırıyor. Makinenizde çalışanlar gerektirir.',
    inContainer: 'konteynerde',
    onThisMachine: 'bu makinede',
    needsConsent:
      'Bu komutlar makinenizde çalışacak ve onaylanmadı. Okuyun, sonra onaylayın — onay tam olarak bu komutlara kaydedilir, yani bir değişiklik yeniden sorar.',
    approved: 'Bu makinede, tam olarak bu komutlar için onaylandı.',
    approve: 'Bu komutları onayla',
    revoke: 'Onayı geri çek',
    policyOff: 'Bu makinede hook\u2019lar bir yönetici tarafından kapatılmış.',
    policyHost:
      'Makinede çalışan komutlar bir yönetici tarafından kapatılmış. Konteynerde çalışan adımlar etkilenmiyor.',
  },

  /**
   * Servis paketi yazma (C-1).
   *
   * `explain` özelliği değil engeli adlandırıyor: paket yazmayı imkânsız
   * kılan şey JSON değil, sha256 defter tutmaydı.
   */
  authoring: {
    title: 'Paket yaz',
    explain:
      'Bir manifest taşıdığı her dosyanın hash’ini yazar ve StackVo bunları her okumada kontrol eder — yani bir fragment’i elle düzenlemek yüklenmeyen bir paket bırakır. Oluştur, baştan doğru olanı yazar; Mühürle, siz düzenledikten sonra hash’leri düzeltir ve doğrulayıcının reddedeceği hiçbir şeyi mühürlemez.',
    category: 'Kategori',
    service: 'Servis kimliği',
    version: 'Sürüm',
    image: 'İmaj',
    imageHint: 'depo:etiket — bir paket çalıştırdığı imajı sabitler. Yalnız oluştururken gerekli.',
    create: 'Oluştur',
    check: 'Denetle',
    seal: 'Mühürle',
    refused: 'Reddedildi — hiçbir şey yazılmadı:',
    valid: '{service} {version} geçerli.',
    resealed: 'Hash’leri yeniden yazılanlar: {files}',
  },

  /**
   * Yerel DNS yanıtlayıcısı (E-1).
   *
   * `explain` ne *olmadığını* söylüyor — bir çözümleyici değil — çünkü kendi
   * makinesinde DNS'e cevap veren bir şeyi açmadan önce insanın ihtiyacı olan
   * bilgi bu.
   */
  perf: {
    title: 'Performans katmanı',
    explain:
      'Bind mount, metadata ve yazmada adlandırılmış birimin 2–3 katına mal oluyor; macOS ve Windows’ta Docker akışını yavaş hissettiren yer burası. Bu dizinleri konteyner içindeki araçlar yazıyor ve her istekte yine onlar okuyor — host dosya sisteminden çıkarmak, framework açılışında 3,8 kat, bir isteğin yazmalarında 2,8 kat ölçüldü. Kendi kodunuz editörünüzün gördüğü yerde kalıyor.',
    inVolume: 'Birimde ({volume})',
    onHost: 'Host’ta — {files}+ dosya',
    notThereYet: 'Projede henüz yok; konteyner içindeki araçlar oluşturacak.',
    editorCannotSee:
      'Editörünüz artık bu dizini göremiyor. Dizin güncellenmesi gerektiğinde bir anlık görüntü dışa aktarın.',
    export: 'Host’a aktar',
    exported:
      '{path} host’a kopyalandı — {size}. Bu bir anlık görüntü; konteyner birime yazmayı sürdürüyor.',
    forget: 'Birimi sil',
    toggle: '{path} dizinini birime taşı',
    needsRecreate: 'Etkili olması için konteynere uygulayın.',
    nothingToOffer: 'Taşınacak bir şey yok — bu projede bağımlılık dizini bulunmuyor.',
  },
  site: {
    title: 'Proje ayarları',
    explain:
      'Bu uygulamanın projenin kendi konteynerine uyguladığı ayarlar. .stackvo/site.json içinde tutulur, yani bir takım arkadaşınız klonladığında onunla birlikte gelir.',
    envTitle: 'Ortam değişkenleri',
    envExplain:
      'Konteynere veriliyor; uygulamanızın .env dosyasına yazılmıyor — o dosya framework’ün. Konteyner yeniden oluşturulunca geçerli olur.',
    key: 'Ad',
    value: 'Değer',
    addRow: 'Değişken ekle',
    removeRow: 'Bu değişkeni kaldır',
    save: 'Kaydet',
    listing: 'Dizin listesi göster',
    listingHint:
      'Index dosyası olmayan yerlerde gezilebilir bir liste sunar. İndirme klasörü ya da derleme çıktısı için işe yarar.',
    listingUnsupported:
      '{server} için yapılandırma dosyası yok — kendi imajının içinde yapılandırılıyor.',
    sshAgent: 'SSH ajanımı ilet',
    sshAgentHint:
      'composer install ve git pull konteynerin içinden özel depolara ulaşabilir; imaja hiçbir anahtar kopyalanmadan. O konteynerde çalışan her şey, konteyner ayakta olduğu sürece anahtarlarınızla imzalayabilir.',
    sshAgentNone: 'Bu makinede çalışan bir SSH ajanı yok, iletilecek bir şey de yok.',
  },
  worktree: {
    title: 'Worktree’ler',
    explain:
      'Bir dala kendi ortamını verin: kendi dizini, kendi adresi, kendi veritabanı. İki dal aynı anda çalışır ve checkout’a git’in fark edeceği hiçbir şey yazılmaz.',
    explainSelf:
      'Bu proje bir worktree — başka bir projenin deposunun ikinci bir checkout’u; kendi dalında ve kendi ortamıyla.',
    new: 'Yeni worktree',
    none: 'Henüz kendi ortamı olan bir dal yok.',
    parent: 'Şu projenin dalı',
    branch: 'Dal',
    branchTaken: 'zaten başka bir yerde checkout edilmiş',
    createBranch: 'Dalı oluştur',
    newBranchName: 'Yeni dal adı',
    nameOverride: 'Ad (isteğe bağlı)',
    domain: 'Cevap verdiği adres',
    database: 'Veritabanı',
    databaseMode: 'Veritabanı',
    dbNone: 'Yok',
    dbCreate: 'Yeni ve boş bir tane',
    dbCopy: 'Bu çalışma alanınınkinin kopyası',
    instance: 'Hangi motorda',
    stopped: 'durdurulmuş',
    noDatabase: 'Yok',
    seededFrom: 'Şuradan kopyalandı',
    copiedFrom: '{source} kopyalanarak',
    willBeCalled: 'Adı şu olacak',
    willAnswerAt: 'Şurada cevap verecek',
    create: 'Oluştur',
    cancel: 'Vazgeç',
    remove: 'Kaldır',
    removeTitle: '{name} kaldırılsın mı?',
    removeExplain:
      'Checkout gider. Aşağıdaki her şey ayrı bir karardır ve siz açmadıkça hiçbiri yapılmaz.',
    removeForce: 'Commit edilmemiş değişikliklerini at',
    removeDatabase: 'Veritabanını sil ({name})',
    removeBranch: 'Dalı sil ({branch})',
    dirty: 'commit edilmemiş değişiklik',
    orphaned: 'dizin yok',
    envTitle: 'Ortam değişkenleri',
    envExplain:
      'Bu worktree’nin konteynerine veriliyor. Checkout’un dışında tutuluyor; çünkü bir worktree’deki dosyalar dalın dosyalarıdır — oraya yazmak birinin git status’ünde görünürdü.',
    derivedExplain:
      'Bunlar da veriliyor ve saklanmıyor: her yeniden üretimde motordan okunuyor — yani Ayarlar’da değişen bir parola, burada hiçbir şey düzenlenmeden bu dala ulaşıyor. Birini değiştirmek için yukarıda aynı adla bir değişken tanımlayın.',
    key: 'Ad',
    value: 'Değer',
    addRow: 'Değişken ekle',
    removeRow: 'Bu değişkeni kaldır',
    saveEnv: 'Kaydet',
  },
  dns: {
    title: 'Yerel DNS',
    subtitle: 'Hosts dosyasını düzenlemeden bu çalışma alanının adlarına cevap ver',
    explain:
      'Tek bir soneke cevap veren, diğer her şeyi reddeden bir yanıtlayıcı. Asla iletmiyor, üst sunucusu ve önbelleği yok — bu makinenin çözümleyicisi değil, yalnız StackVo’nun ürettiği adların. Joker adları da bu çalıştırıyor; hosts dosyası bunu hiç yapamıyor.',
    responder: '127.0.0.1:{port} üzerinde cevap ver',
    responderHint: '{suffix} ile biten her ad bu makineye çözülür, proje başına kayıt gerekmeden.',
    udpOnly:
      'Yalnız UDP — tcp/{port} başka bir şeyin elinde. Sorguların çoğu çalışır; TCP üzerinden bir yeniden deneme çalışmaz.',
    broken:
      'Bu makine {suffix} için {port} portuna soruyor ve orada cevap veren yok; o adlar şu anda çözülmüyor. Ya yanıtlayıcıyı açın ya aşağıdaki anahtarı kapatın.',
    stale:
      'Bu çalışma alanının artık kullanmadığı bir sonekten kalmış: {files}. O adlar çözülmüyor, reddediliyor. Aşağıdaki anahtarı yeniden uygulamak bunları kaldırır.',
    foreign:
      'O yolda zaten bir dosya var ve bize ait değil — {detail}. Önce kenara kopyalanır, bu kapatıldığında geri konur.',
    resolver: 'Sistem buna sorsun',
    resolverHint:
      '{mechanism} üzerinden {file} dosyasını yazar. Yönetici parolası ister ve bu makinenin o soneki nasıl çözdüğünü değiştirir.',
    resolverHintRule:
      'Bu sonek için bir {mechanism} kuralı ekler. Yönetici parolası ister ve bu makinenin o soneki nasıl çözdüğünü değiştirir.',
    reload: 'Ardından şunu çalıştırır: {command}',
    manual:
      'Bu makinenin çözümleyicisinin önünde tanınan bir şey yok, dolayısıyla sizin için yazılacak bir dosya da yok. Bu satırı burada adları gerçekten çözen şeye — dnsmasq, NetworkManager — ekleyip yeniden yükleyin:',
    manualFile: 'Çoğu makinede o dosya {file} olur.',
    noPrompt:
      'Bu makinede {mechanism} var ama pencereli bir uygulamanın parola sorabileceği bir yol yok. Bunu kendiniz uygulayın:',
    test: 'Sına',
    testHint: 'Önce yanıtlayıcıya, sonra bu makineye sorar — bunlar farklı sorular.',
    mechanisms: {
      resolver: 'bir /etc/resolver dosyası',
      'network-manager': 'NetworkManager’ın dnsmasq’ı',
      dnsmasq: 'dnsmasq',
      'systemd-resolved': 'systemd-resolved',
      nrpt: 'Ad Çözümleme İlke Tablosu (NRPT)',
      manual: 'bilinen bir mekanizma yok',
    },
    probes: {
      udp: 'Yanıtlayıcı, UDP üzerinden',
      tcp: 'Yanıtlayıcı, TCP üzerinden',
      system: 'Bu makinenin kendi çözümleyicisi',
      public: 'İnternetin geri kalanı',
    },
  },

  /**
   * Kullanıcı rotaları (E-4).
   *
   * `explain` `localhost` ile başlıyor, çünkü herkesin yazdığı ve yardımsız
   * çalışamayan tek şey o.
   */
  routes: {
    title: 'Özel rotalar',
    subtitle: 'Bir adı StackVo’nun başlatmadığı bir şeye yönlendirin',
    explain:
      'Kendi başlattığınız bir dev sunucusu, başka bir araçtaki servis, bir staging adresi. http://localhost:3000 yazın, StackVo düzeltir — proxy’nin konteynerinin içinde “localhost” proxy’nin kendisidir, ki bu açıklamasız bir 502 demektir.',
    domain: 'Ad',
    target: 'Şuraya gider',
    enabled: 'Etkin',
    add: 'Rota ekle',
    remove: 'Bu rotayı kaldır',
    save: 'Kaydet ve uygula',
    empty: 'Henüz özel rota yok.',
  },

  /**
   * Bir örneğin verisini diğerine taşıma (G-4).
   *
   * `explain` yıkıcı yarıyla başlıyor, çünkü planın var olma nedeni tam da bu
   * bilgiyi düğmeye basmaya değer olmadan önce ekrana koymak.
   */
  dbMove: {
    title: 'Veriyi başka bir örneğe taşı',
    explain:
      'Bu örneği döker ve bir diğerine geri yükler; oradaki her şeyin yerine geçer. Aynı motor çalışır; MySQL ve MariaDB birbirini dikkatle okur; farklı aileler reddedilir.',
    target: 'Şuraya',
    move: 'Taşı',
    confirm: '{to} içindeki her şey bu örneğin içeriğiyle değiştirilsin mi?',
    done: '{to} içine {bytes} bayt taşındı.',
  },

  /**
   * Boştaki projeleri askıya alma (I-2).
   *
   * `explain` sinyali adlandırıyor, çünkü kendi başına konteyner durduran bir
   * şey hakkındaki ilk soru "nereden biliyor".
   */
  idle: {
    title: 'Boştaki projeleri askıya al',
    subtitle: 'Kimsenin istemediğini durdur',
    explain:
      'Proxy’nin erişim günlüğünden ölçülüyor — tek dürüst sinyal, çünkü php-fpm hizmet verirken de uyurken de CPU kullanmıyor. Askıya alınan proje sadece durdurulmuş olur; listeden, tepsiden ya da ⌘K’dan başlatın. İstek üzerine uyandırma yok.',
    threshold: 'Boşta dakika',
    thresholdHint: '0 bunu kapatır. Günlüğün hiç anmadığı bir proje asla askıya alınmaz.',
    suspendNow: '{count} tanesini şimdi askıya al',
    none: 'Çalışan proje yok.',
    never: 'henüz istek yok',
    justNow: 'az önce',
    minutes: '{minutes} dk önce',
    wouldStop: 'eşiği geçti',
  },

  quickActions: {
    startAll: 'Tüm konteynerleri başlat',
    stopAll: 'Tüm konteynerleri durdur',
    restart: 'Tüm konteynerleri yeniden başlat',
  },

  dashboard: {
    subtitle: 'Yığının ve makinenin anlık durumu',
    title: 'Panel',
    overview: 'Genel Bakış',
    health: 'Sağlık',
    projects: 'Projeler',
    services: 'Servisler',
    images: 'İmajlar',
    running: 'Çalışıyor',
    stopped: 'Durdu',
    active: 'Aktif',
    inactive: 'Pasif',
    cpuLoad: 'İşlemci Yükü',
    cpuHistory: 'İşlemci Geçmişi',
    cpu: 'CPU',
    system: 'Sistem',
    user: 'Kullanıcı',
    nice: 'Nice',
    idle: 'Boşta',
    used: 'Kullanılan',
    available: 'Boş',
    min: 'En az',
    avg: 'Ortalama',
    max: 'En çok',
    diskIo: 'Disk G/Ç',
    diskIoSub: 'Anlık disk okuma/yazma',
    read: 'Okuma',
    write: 'Yazma',
    readHistory: 'Okuma geçmişi',
    writeHistory: 'Yazma geçmişi',
    network: 'Ağ Trafiği',
    networkSub: 'Anlık ağ kullanımı',
    downloadHistory: 'İndirme geçmişi',
    uploadHistory: 'Yükleme geçmişi',
    free: 'Boş',
  },

  projectsView: {
    worktreeOf: '{parent} dalı',
    colFavourite: 'Favori',
    subtitle: 'Yönetilen projeler ve konteynerleri',
    title: 'Projeler',
    list: 'Proje Listesi',
    running: 'Çalışıyor',
    searchPlaceholder: 'Proje ara...',
    colDomain: 'Alan Adı',
    colRuntime: 'Çalışma Ortamı',
    colRepo: 'Repo',
    filter: {
      all: 'Hepsi',
      running: 'Çalışan',
      stopped: 'Durmuş',
      unbuilt: 'Derlenmemiş',
      favourites: 'Yalnızca favoriler',
      title: 'Süzgeçler',
      status: 'Durum',
      clear: 'Süzgeçleri temizle',
    },
    repoLocal: 'Git deposu — uzak sunucusu yok',
    colServer: 'Sunucu',
    colConfiguration: 'Yapılandırma',
    colStopStart: 'Durdur/Başlat',
    colRestart: 'Yeniden Başlat',
    rebuild: 'Yeniden derle',
    colTerminal: 'Terminal',
    colOpen: 'Tarayıcıda aç',
    colDetail: 'Detay',
    colDelete: 'Sil',
    colMore: 'İşlemler',
    // Satır sonundaki üç nokta menüsü. Sütun başlıkları bir sütunu adlandırır
    // ("Durdur/Başlat"); bunlar tek bir eylemi adlandırır, çünkü menüde o an
    // yapılabilecek olan hangisiyse yalnızca o görünür.
    menu: {
      build: 'Derle',
      start: 'Başlat',
      stop: 'Durdur',
      restart: 'Yeniden başlat',
      apply: 'Değişiklikleri uygula',
      fixHosts: 'hosts kaydını ekle',
    },
    // Her biri projenin adını taşıyor. Yirmi satırlık bir tabloda her butonun
    // "Sil" demesi, ekran okuyucu kullanıcısına hangi projeyi kaldırmak üzere
    // olduğunu söylemez.
    aria: {
      favourite: '{name} projesini yukarı sabitle',
      unfavourite: '{name} sabitlemesini kaldır',
      build: '{name} projesini derle',
      stop: '{name} projesini durdur',
      start: '{name} projesini başlat',
      restart: '{name} projesini yeniden başlat',
      open: '{name} projesini tarayıcıda aç',
      detail: '{name} projesinin ayrıntılarını aç',
      fixHosts: '{name} için hosts kaydı ekle',
      more: '{name} için işlemler',
    },
    default: 'Varsayılan',
    noDnsRecord: 'hosts kaydı yok',
    addToHosts: 'hosts dosyasına ekleyin:',
  },

  catalogueSettings: {
    title: 'Katalog',
    desc: 'Servis paketlerinin nereden çekildiği, ve o adresin çalışıp çalışmadığı',
    current: '{location} · {packages} paket yayımlanmış, bu makinede {installed} sürüm kurulu',
    none: 'Bu makinede henüz katalog yok. StackVo hiçbir servisi kendi içinde taşımıyor, yani biri çekilene kadar hiçbir şey kullanılabilir değil.',
    policyBundle: 'Bir yönetici kaynağı {path} paketine sabitlemiş. Aşağıdaki adres yok sayılıyor.',
    policyMirror: 'Bir yönetici kaynağı {url} adresine sabitlemiş. Aşağıdaki adres yok sayılıyor.',
    signatureRequired:
      'Bu makine imzalı katalog istiyor ve henüz yayımlanmış bir imzalama anahtarı yok; çekme, imzasıza düşmek yerine reddediliyor.',
    address: 'Katalog adresi',
    addressHint:
      'Bir https:// adresi ya da bir klasör. GitHub depo adresi, dosyaların gerçekte sunulduğu yere çevrilir.',
    test: 'Test et',
    pickFolder: 'Klasör seç',
    use: 'Çek ve kullan',
    ok: 'Erişilebilir — {packages} paket, {versions} sürüm, indeks {sequence}.',
    backwards:
      'Bu indeks {sequence}, burada olan {current}. Çekmek reddedilirdi: geriye giden bir indeks, geri çekilmiş bir sürümün geri gelme yoludur.',
    failed: 'Orada bir katalog okunamadı',
    resolved: '{url} adresinden çekildi',
    bundleTitle: 'Hava boşluklu paket',
    bundleWhat:
      'Bu katalogu ve bütün paketleri tek klasöre yazın, ağı olmayan bir makineye götürmek için. O makineyi bu klasöre yöneltin — StackVo içinde hiçbir servis taşımıyor, yani bir katalogun oraya ulaşmasının başka yolu yok.',
    bundleAction: 'Paket yaz…',
    bundleNeedsCatalogue:
      'Önce bir katalog çekin — paket, bu makinenin kullandığı katalogun kopyasıdır.',
    bundleDone: 'Yazıldı: {packages} paket, {versions} sürüm, {files} dosya, {size}.',
    bundleUnsigned:
      'Yanında imza gitmedi. İmzalı katalog şart koşan bir makine bu paketi reddeder.',
    bundleSkipped: 'Taşınmadı, çünkü yayıncı geri çekti:',
    bundleNext:
      'Öteki makinede bu klasörü katalog adresi olarak seçin — ya da market.offlineBundle’ı ona ayarlayın.',
  },
  marketView: {
    createTitle: 'Yeni instance: {id}',
    createBody:
      'Bunlar paketin kendi varsayılanları. Şimdi değiştirmeye değen kısım kimlik bilgileri: bir imaj root parolasını yalnızca boş bir veri dizinini ilk kurarken okur, yani ayarlanabileceği tek an burası.',
    createNoPort: '{handles} için boş port bulunamadı — kendiniz seçin.',
    search: 'Katalogda ara',
    title: 'Katalog',
    subtitle: 'Servisler nereden geliyor, ve bu makinede hangi sürümler var',
    chooseSource: 'Bir kaynak seçin',
    sourceTitle: 'Katalog nereden geliyor',
    sourceCounts: '{packages} paket yayında, {installed} sürüm kurulu',
    unsigned: 'imza doğrulanmıyor',
    sourceInSettings: 'Ayarlar → Katalog bu adresi tutuyor ve çekmeden test edebiliyor.',
    noCatalogue: 'Henüz katalog yok',
    noCatalogueBody:
      'StackVo içinde hiçbir servis taşımıyor. Bir kaynak gösterin — çevrimdışı bir paket ya da servis paketleri deposunun bir kopyası — katalog oradan okunur.',
    available: 'Yayında olanlar',
    availableDesc: 'Kaynağın yayımladıkları, ve bu makinede hangi sürümler var',
    showOlder: 'Desteği bitmiş sürümleri göster',
    multiVersion: 'Birden çok sürüm çalıştırır',
    versionCount: '{n} sürüm',
    hiddenCount: '{n} desteği bitmiş',
    serviceCount: '{n} servis',
    eolWhy:
      'Desteği bitmiş sürümler çalışmaya devam eder — üretici yama vermeyi bırakmıştır, bu bozuk olmakla aynı şey değildir. Katalogdan değil, aşağıdaki listelerden tutulurlar: .env’inde o sürüm yazan bir çalışma alanının göç edebilmesi gerekiyor, ve bir sürümü düşürebilen bir indeks, birinin çalışan servisinin kaynağını kaybettiği indekstir.',
    recommended: 'Önerilen',
    supportUntil: 'Destek bitişi: {date}',
    support: {
      supported: 'Destekli',
      deprecated: 'Kullanımdan kalkıyor',
      eol: 'Desteği bitti',
    },
    install: 'Kur',
    uninstall: 'Kaldır',
    addInstance: 'Örnek ekle',
    inUse: 'Bu sürümü bir örnek kullanıyor',
    instances: 'Servis örnekleri',
    instancesDesc:
      'Bu çalışma alanının çalıştırdığı sürümler; her biri kendi verisi ve kendi portuyla',
    noInstances: 'Henüz kurulu bir şey yok',
    noInstancesBody:
      'Yukarıdan bir paket kurun, sonra ondan bir örnek ekleyin. Bir servisin iki sürümü yan yana çalışabilir; her biri kendi verisi ve kendi portuyla.',
    colInstance: 'Örnek',
    colContainer: 'Konteyner Adı',
    colStopStart: 'Durdur/Başlat',
    colRestart: 'Yeniden Başlat',
    colOpen: 'Tarayıcıda aç',
    colStatus: 'Durum',
    enabled: 'ETKİN',
    disabled: 'DEVRE DIŞI',
    stop: 'Durdur',
    start: 'Başlat',
    restart: 'Yeniden başlat',
    primary: 'Birincil',
    packageMissing: 'Paket yok',
    makePrimary: 'Birincil yap',
    removeInstance: 'Kaldır',
    instanceSettings: 'Ayarlar',
    detail: 'Detay',
    handoverTitle: 'Bu çalışma alanı servislerini hâlâ .env içinde tutuyor',
    handoverBody:
      "{n} servis örnek tablosuna taşınacak. Volume'ler yeniden adlandırılmaz, sahiplenilir — veri olduğu yerde kalır; portlar korunur; ve eski container adı ağ takma adı olarak yaşamaya devam eder, yani stackvo-mysql'e bakan bir proje çalışmaya devam eder.",
    handoverBlocked: 'Göç ya tamamı ya hiçbiri, ve şu an çalışamaz. Hiçbir şey değiştirilmedi:',
    handoverRevert: 'Geri alınabilir — .env önce yedeklenir ve anahtarları korunur.',
    handoverRevertHow:
      ".env, hiçbir şey yazılmadan önce .env.pre-market.bak'a kopyalanır ve servis anahtarları silinmez, işaretlenir. Geri dönmek için services/instances.json dosyasını silin.",
    handoverApply: 'Taşı',
    handoverMissing:
      'Göç, .env’in adlandırdığı her sürüm için bir pakete ihtiyaç duyuyor ve {n} tanesi henüz bu makinede değil:',
    handoverInstallAll: 'Kur',
    handoverNotInCatalogue:
      '{subject} bu makinenin okuduğu katalogda da yok. Kaynağı kontrol edin, ya da .env’i katalogda olan bir sürüme çevirin.',
    handoverNote: {
      resolvedMovingTag: '{subject}: hareketli etiket somut bir sürüme sabitleniyor ({detail})',
      portMoved: "{subject}: .env'deki port bu makinede dolu ({detail})",
      adoptedVolume: "{subject}: mevcut volume'ünü koruyor — {detail}",
      settingHasNoHome: '{subject}: {detail} ayarının pakette karşılığı yok',
      unknownService: "{subject} .env'de açık ve katalog onu hiç tanımıyor",
      versionNotInstalled:
        '{subject} için bu makinede paket yok, ve yakın bir sürüme göç ettirilmeyecek — o, kimsenin istemediği bir yükseltmeyi bir veritabanının üzerinde yapmak olurdu. Kurulu olan: {detail}',
      nothingToInstall: '{subject} açık ama katalogda somut bir sürümü yok',
      noFreePort: '{subject}: {detail} için boş port bulunamadı',
    },
  },
  servicesView: {
    companionLogs: '{name} logu',
    alias: 'Ayrıca şu adla erişilir',
    companions: 'Yardımcı konteynerler',
    companionsSubtitle:
      'Bu servisle birlikte gelir, ayrıca kurulamaz. Instance başına adlandırılırlar; iki Kafka tek bir Zookeeper paylaşmaz, iki tane olur.',
    notCreatedShort: 'Oluşturulmadı',
    runtime: 'Çalışma',
    image: 'İmaj',
    imageSize: 'İmaj boyutu',
    uptime: 'Çalışma süresi',
    restarts: 'Yeniden başlatma',
    restartsWithPolicy: '{n} (yeniden başlatma politikası: {policy})',
    exitCode: 'Çıkış kodu',
    exitOutOfMemory: '{code} — öldürüldü, çoğunlukla bellek yetersizliği',
    hide: 'Değeri gizle',
    colDetail: 'Detay',
    serviceInfo: 'Servis bilgisi',
    logInfo: 'Log ve bağlamalar',
    ipAddress: 'IP adresi',
    network: 'Ağ',
    gateway: 'Ağ geçidi',
    portMappings: 'Port eşlemeleri',
    internal: 'yalnızca iç ağ',
    connection: 'Bağlantı dizesi',
    connectionSubtitle:
      'Bir servisin iki adresi vardır. Konteyner adı yalnızca Docker ağının içinde çözülür — bu makinedeki bir istemcinin yayınlanan portu kullanması gerekir.',
    fromHost: 'Bu makineden',
    fromHostHint: 'Compass, TablePlus, psql',
    fromContainer: 'Başka bir konteynerden',
    fromContainerHint: 'projenizin kendi uygulaması',
    openInClient: 'Bir veritabanı istemcisinde aç',
    notPublished:
      'Konteyner çalışıyor ama host tarafına hiçbir port yayınlamıyor; bu makineden erişilemez.',
    credentials: 'Kimlik bilgileri',
    noCredentials: 'Bu paketin yapılandırılacak bir ayarı yok.',
    health: {
      healthy: 'Sağlıklı',
      unhealthy: 'Sağlıksız',
      starting: 'Başlıyor',
    },
    reveal: 'Değeri göster',
    containerLogs: 'Konteyner logu',
    logPath: 'Log yolu',
    mount: 'Bağlama',
    noMounts: 'Bağlama yok.',
    notCreated: 'Konteyner henüz oluşturulmadı.',
    colContainerName: 'Konteyner Adı',
    colDomain: 'Alan Adı',
    networkInfo: 'Ağ Bilgisi',
    dependencies: 'Bağımlılıklar',
    noDependencies: 'Bağımlılığı yok.',
    required: 'Zorunlu',
    optional: 'İsteğe bağlı',
    depRunning: 'çalışıyor',
    depStopped: 'çalışmıyor',
    depNotInstalled: 'bunu sağlayan kurulu bir şey yok',
  },

  projectDetail: {
    subtitle: 'Tek bir proje: neyi çalıştırdığı, neyden kurulduğu ve şu an ne yaptığı.',
    debug: 'Hata ayıklama',
    runtime: 'Çalışma zamanı ayarları',
    title: 'Proje Detayı',
    back: 'Geri',
    indicator: 'Gösterge',
    configuration: 'Yapılandırma',
    container: 'Konteyner',
    live: 'Canlı — kaynak ölçümleri 2 saniyede bir yenilenir',
    disk: 'Disk',
    composition: 'Dağılım',
    usedShort: 'kullanımda',
    cpuActivity: 'İşlemci Aktivitesi',
    noHistory: 'Henüz geçmiş yok — ölçümler dakikada bir alınıyor.',
    noSample: 'ölçüm yok',
    less: 'Az',
    more: 'Çok',
    sslStatus: 'SSL Durumu',
    sslEnabled: 'Etkin (HTTPS)',
    type: 'Tür',
    containerPath: 'Konteyner Yolu',
    hostPath: 'Host Yolu',
    accessHttp: 'Erişim URL · HTTP',
    accessHttps: 'Erişim URL · HTTPS',
    phpExtensions: 'PHP Eklentileri',
    name: 'Ad',
    uptime: 'Başlangıç',
    created: 'Oluşturulma',
    restartPolicy: 'Yeniden Başlatma Politikası',
    restartCount: 'Yeniden Başlatma Sayısı',
    containerId: 'Konteyner ID',
    imageSize: 'İmaj Boyutu',
    dnsHosts: 'DNS (HOSTS)',
    configured: 'Yapılandırıldı',
    gateway: 'Ağ Geçidi',
    portMappings: 'Port Eşlemeleri',
    notPublished: 'yayınlanmadı',
    copied: 'Kopyalandı',
    applyToContainer: 'Konteyneri yeniden oluştur',
  },

  workspace: {
    none: 'Henüz bir proje dizini seçilmedi.',
    change: 'Değiştir',
    source: {
      stored: 'kayıtlı seçim',
      env: 'STACKVO_PROJECTS değişkeni',
      migrated: 'eski kurulumdan taşındı',
      none: 'seçilmedi',
    },
    version: 'Sürüm',
    appDir: 'Uygulama dizini',
    appDirDesc:
      'StackVo’nun kendi ürettiği her şey burada: compose dosyaları, loglar, sertifikalar, ayarlar. Otomatik oluşturulur, sorulmaz.',
  },

  engine: {
    title: 'Docker motoru',
    running: 'Çalışıyor',
    down: 'Çalışmıyor',
    socket: 'Soket',
    context: 'Bağlam',
    version: 'Sürüm',
    apiVersion: 'API sürümü',
    platform: {
      'docker-desktop': 'Docker Desktop',
      colima: 'Colima',
      orbstack: 'OrbStack',
      engine: 'Docker Engine',
      unknown: 'Bilinmiyor',
    },
  },

  stats: {
    cpu: 'İşlemci',
    memory: 'Bellek',
    storage: 'Disk',
    network: 'Ağ',
    cores: 'çekirdek',
    download: 'İndirme',
    upload: 'Yükleme',
    inUse: 'kullanımda',
    unused: 'kullanılmıyor',
  },

  projects: {
    searchPlaceholder: 'Proje ara…',
    openDetail: 'Detayı aç',
    openSite: 'Siteyi aç',
    title: 'Projeler',
    empty: 'Henüz proje yok',
    emptyText:
      'Proje dizininizde StackVo’nun yönettiği bir proje bulunmuyor. Yeni bir tane oluşturun ya da mevcut bir klasörü buraya taşıyıp sahiplendirin.',
    noMatch: 'Eşleşen proje yok',
    noMatchText: '“{term}” aramasıyla eşleşen bir proje bulunamadı.',
    noMatchFilter: 'Seçili süzgeçlere uyan bir proje yok.',
    clearSearch: 'Aramayı ve süzgeçleri temizle',
    running: 'Çalışıyor',
    stopped: 'Durdu',
    notBuilt: 'Derlenmedi',
    domainMissing: 'hosts kaydı yok',
    domainMissingHint: 'Bu alan adı /etc/hosts dosyasında yok, tarayıcıdan açılmaz.',
    invalidManifest: 'Geçersiz stackvo.json',
    problems: 'sorun',
    manifestChanged: 'stackvo.json değişti — yeniden üretilmeli.',
    manifestChangedBuilt:
      'stackvo.json değişti. Konteyner hâlâ derlendiği imajı çalıştırıyor — yeniden üretmek, imajı derlemek ve konteyneri yeniden yaratmak için tıklayın.',
    openFolder: 'Klasörü aç',
  },

  services: {
    hostPort: 'Host portu',
    unmetDependency: 'Eksik bağımlılık',
  },

  console: {
    doneToast: '{operation} tamamlandı — {duration}',
    failedToast: '{operation} başarısız — çıktı konsolda',
  },

  catalogueGate: {
    title: 'Bu makinede henüz servis kataloğu yok',
    body: 'StackVo hiçbir servisi kendi içinde taşımıyor — ne şablon, ne de listenin bir kopyası. Yani bu boş bir katalog değil: burada henüz hiç yok, ve bir servis kurulabilmesi için bir yerden gelmesi gerekiyor.',
    signatureRequired:
      'Bu makine imzalı katalog istiyor ve henüz yayımlanmış bir imzalama anahtarı yok. Çekme, imzasıza düşmek yerine reddediliyor — sessizce hiçbir şey yapmayan bir kontrol, hiç olmayandan kötüdür.',
    policyBundle: 'Bir yönetici kaynağı {path} paketine sabitlemiş. İki düğme de onu kullanıyor.',
    policyMirror: 'Bir yönetici kaynağı {url} adresine sabitlemiş. İki düğme de onu kullanıyor.',
    online: 'İnternetten çek',
    onlineBody:
      'HTTPS üzerinden indirilip önbelleğe alınır. Bir kez geldikten sonra kalır ve uygulama çevrimdışı çalışır.',
    address: 'Katalog adresi',
    fetch: 'Katalogu çek',
    offline: 'Bu makinede internet yok',
    offlineBody:
      'Bir hava boşluğu paketini ya da servis paketleri deposunun bir kopyasını gösterin. Bu, hava boşluklu kurulumun yedek yolu değil, cevabın kendisi.',
    choose: 'Klasör seç',
    skip: 'Servissiz devam et',
    skipHint:
      'Projeler, ters vekil ve sertifikalar katalog olmadan da çalışır. Market sayfası bu iki seçeneği istediğiniz zaman yeniden sunar.',
  },
  bootstrap: {
    title: 'Yığın hazırlanıyor',
    subtitle:
      'Bir defalık kurulum: compose dosyaları yazılıyor ve çekirdek konteynerler ayağa kaldırılıyor. Bittiğinde stackvo.loc yayında olacak.',
    generate: 'Compose dosyaları yazılıyor',
    generateDetail: 'Şablonlar ayarlarınızla işleniyor; up komutuna verilecek dosyalar bunlar.',
    start: 'Çekirdek konteynerler başlatılıyor',
    startDetail:
      'Traefik — her alan adının üzerinden geçtiği vekil sunucu. İlk seferde imaj indirilebilir.',
    certificates: 'Sertifika üretiliyor',
    certificatesDetail:
      'Traefik HTTPS sunuyor; sertifika olmadan hiçbir alan adı cevap veremez. İlk seferde sistem parolası sorulabilir.',
    trust: 'Sertifikaya güven',
    trustDetail:
      'macOS bu izni yalnızca etkileşimli veriyor, o yüzden bir terminal açılır ve sudo parolanız sorulur. Girmezseniz yığın yine çalışır, tarayıcı sadece uyarı gösterir.',
    waitingForPassword: 'Terminal açıldı — parolanızı girin, buradan takip ediliyor.',
    retry: 'Yeniden dene',
    untrusted:
      'Sertifika üretildi ama sistem ona güvenmiyor — tarayıcı uyarı gösterecek. Ayarlar → Sertifikalar’dan tekrar deneyebilirsiniz.',
  },

  preflight: {
    title: 'StackVo çalışmaya hazır değil',
    subtitle: '{count} gereksinim karşılanmıyor. Uygulama, bunlar tamamlanınca açılacak.',
    recheck: 'Yeniden denetle',
    blocked: 'Yukarıdaki bir gereksinim karşılanmadan denetlenemiyor.',
    lead: 'Adımları sıradan takip edin — işaretli adımın düğmesi işi uygulama adına yapar.',
    progress: '{total} adımın {done} tanesi tamam',
    nextStep: 'Sıradaki adım',
    manual: 'Bu adımı elle tamamlamanız gerekiyor.',
    help: 'Kurulum talimatları',

    workspace: 'Proje dizini',
    workspaceHint: {
      macos:
        'Projelerinizin durduğu klasörü seçin — mevcut bir klasör de olabilir, yeni bir tane de. Docker’ın erişebildiği bir yerde olmalı; ev dizininizin altı güvenlidir. StackVo kendi dosyalarını buraya değil, ~/.stackvo altına yazar.',
      linux:
        'Projelerinizin durduğu klasörü seçin — mevcut bir klasör de olabilir, yeni bir tane de. Docker’ın erişebildiği bir yerde olmalı; ev dizininizin altı güvenlidir. StackVo kendi dosyalarını buraya değil, ~/.stackvo altına yazar.',
      windows:
        'Projelerinizin durduğu klasörü seçin — mevcut bir klasör de olabilir, yeni bir tane de. Docker Desktop’ın paylaştığı bir sürücüde olmalı. StackVo kendi dosyalarını buraya değil, kendi dizinine yazar.',
    },
    workspaceAction: 'Proje dizinini seç',
    workspaceInstalled: 'Proje dizini {path} olarak ayarlandı.',

    engine: 'Docker motoru',
    engineHint: {
      macos:
        'Docker Desktop, OrbStack veya Colima çalışmıyor. Başlat düğmesi Docker Desktop’ı açar.',
      linux:
        'Docker daemon çalışmıyor. Başlat düğmesi systemd üzerinden dener; yetki gerekirse `sudo systemctl start docker`.',
      windows:
        'Docker Desktop çalışmıyor. Başlat düğmesi onu açar; WSL2 arka ucunun kurulu olması gerekir.',
    },
    engineAction: 'Başlat',

    compose: 'Docker Compose v2',
    composeHint: {
      macos:
        'Uygulama compose profilleri kullanıyor; bunlar v2 ile geldi. Docker Desktop’ı güncelleyin.',
      linux:
        'Uygulama compose profilleri kullanıyor; bunlar v2 ile geldi. docker-compose-plugin paketini kurun.',
      windows:
        'Uygulama compose profilleri kullanıyor; bunlar v2 ile geldi. Docker Desktop’ı güncelleyin.',
    },

    network: 'Paylaşılan Docker ağı',
    networkHint: {
      macos:
        'Üretilen compose dosyaları bu ağı “external” olarak bildiriyor, yani compose onu kendisi oluşturmaz.',
      linux:
        'Üretilen compose dosyaları bu ağı “external” olarak bildiriyor, yani compose onu kendisi oluşturmaz.',
      windows:
        'Üretilen compose dosyaları bu ağı “external” olarak bildiriyor, yani compose onu kendisi oluşturmaz.',
    },
    networkAction: 'Ağı oluştur',

    hosts: 'Alan adı kayıtları',
    hostsHint: {
      macos:
        'Bu adlar /etc/hosts dosyasında yok, dolayısıyla tarayıcı hiçbirini çözemez. Ekleme yönetici parolası ister; ne yazılacağı önce gösterilir.',
      linux:
        'Bu adlar /etc/hosts dosyasında yok, dolayısıyla tarayıcı hiçbirini çözemez. Ekleme yönetici parolası ister; ne yazılacağı önce gösterilir.',
      windows:
        'Bu adlar Windows\\System32\\drivers\\etc\\hosts dosyasında yok, dolayısıyla tarayıcı hiçbirini çözemez. Ekleme yönetici izni ister; ne yazılacağı önce gösterilir.',
    },
    hostsAction: 'Kayıtları ekle',

    mkcert: 'mkcert',
    mkcertHint: {
      macos:
        'SSL açık, yani her alan adı HTTPS üzerinden sunuluyor. mkcert olmadan sertifika üretilmez ve tarayıcılar siteyi açmayı reddeder. `brew install mkcert` ile kurun.',
      linux:
        'SSL açık, yani her alan adı HTTPS üzerinden sunuluyor. mkcert olmadan sertifika üretilmez ve tarayıcılar siteyi açmayı reddeder. Paket yöneticinizden kurun, sonra `mkcert -install` çalıştırın.',
      windows:
        'SSL açık, yani her alan adı HTTPS üzerinden sunuluyor. mkcert olmadan sertifika üretilmez ve tarayıcılar siteyi açmayı reddeder. `choco install mkcert` ile kurun.',
    },
  },
  imports: {
    found: '{tool} içinde bulundu: {n} site',
    explain:
      '{path} okundu. Oraya asla bir şey yazılmaz. İçe aktarma, siteyi bu çalışma alanına kopyalar ve ardından diğer klasörler gibi sahiplenir.',
    take: 'İçe aktar',
    taken: 'Zaten burada',
    serviceHint:
      'Compose dosyası bunu istiyor. StackVo’nun kendisi var — içe aktardıktan sonra Ayarlar’dan açın.',
    move: 'Kopyalamak yerine taşı',
    moveOff: 'Aslı yerinde kalır, yani karşılaştırırken diğer araç çalışmaya devam eder.',
    moveOn: 'Kopya tamamlandığında aslı silinir. Diğer araç bu siteyi artık sunmayacak.',
    pick: '{tool} klasörünü göster',
    notThere: 'O klasör bir {source} kurulumuna benzemiyor.',
    sizeAtLeast: 'en az {size}',
    colSite: 'Site',
    colDetected: 'Algılanan',
    colDomain: 'Alan adı',
    colSize: 'Boyut',
    colAction: 'İçe aktar',
  },
  unmanaged: {
    title: 'Sahiplenilmemiş kod',
    review: 'Sahiplenilecek klasörler ve siteler',
    explain:
      'Bu makinede olup StackVo’nun çalıştırmadığı kod: proje klasörünüzde stackvo.json dosyası olmayan klasörler ve XAMPP ya da Laragon’a ait siteler.',
    waiting: '{n} tane bekliyor.',
    nothing: 'Bekleyen bir şey yok.',
    pickExplain: 'Yalnızca alışılmış kurulum yolları tarandı. Başka bir yeri gösterin.',
    none: 'Bir şey bulunamadı. Proje klasörünüzdeki her klasörün stackvo.json dosyası var ve bu araçların normalde kurulduğu yerlerde XAMPP ya da Laragon sitesi görülmedi.',
  },
  adopt: {
    found: 'Buradaki {n} klasörün stackvo.json dosyası yok.',
    where: '{path} içinde tarandı',
    colFolder: 'Klasör',
    colDetected: 'Algılanan',
    colEvidence: 'Neye göre algılandı',
    colAction: 'Sahiplen',
    from: '{files} dosyasından algılandı',
    noEvidence: 'tanınan bir şey yok — varsayılanlar kullanılacak',
    action: 'Sahiplen',
  },
  migrate: {
    read: 'Compose’u oku',
    title: '{name} projesini compose dosyasından içe aktar',
    project: 'Proje',
    field: {
      runtime: 'Çalışma ortamı',
      server: 'Sunucu',
      phpVersion: 'PHP sürümü',
      nodeVersion: 'Node sürümü',
      documentRoot: 'Belge kökü',
      domain: 'Alan adı',
      extensions: 'PHP eklentileri',
    },
    services: 'Etkinleştirilecek servisler',
    servicesAlready: 'Bu projenin ihtiyaç duyduğu servisler zaten etkin.',
    unmapped: 'StackVo karşılığı yok — bunları kendiniz ele almanız gerekecek:',
    alreadyManaged: 'Bu projenin zaten bir stackvo.json dosyası var; yalnızca servisler değişecek.',
    evidence: 'Her yanıtın okunduğu yer',
    manifest: 'Yazılacak stackvo.json',
    apply: 'İçe aktar',
  },
  mail: {
    subtitle: 'Projelerinin gönderdiği postalar, makineden çıkmadan yakalanmış hâlde.',
    inbox: 'Gelen kutusu',
    title: 'Mail',
    unread: '{n} okunmamış',
    select: 'Okumak için bir mesaj seçin.',
    fromLabel: 'Kimden',
    toLabel: 'Kime',
    replyToLabel: 'Yanıt adresi',
    offHeadline: 'Mail yakalayıcı kapalı',
    stoppedHeadline: 'Mail yakalayıcı duruyor',
    emptyHeadline: 'Henüz mail yok',
    preview: 'Önizleme',
    text: 'Metin',
    source: 'Kaynak',
    headersTab: 'Başlıklar',
    attachmentsTab: 'Ekler',
    compatTab: 'Uyumluluk',
    linksTab: 'Bağlantılar',
    save: 'Kaydet',
    searchPlaceholder: 'Ara — from:a{\'@\'}b.c subject:"fatura"',
    matching: '{n} eşleşme',
    compatSupported: '{n} mail istemcisi özelliğinde tam destekleniyor',
    compatLegend: 'Yeşil tam destek · turuncu kısmi · kırmızı desteklenmiyor.',
    compatWarning: '{category} · {found}× geçiyor',
    compatClean: 'Bu işaretlemede test edilen hiçbir yerde desteklenmeyen şey yok.',
    checkLinks: 'Bağlantıları denetle',
    linksHint: 'Mesajdaki her bağlantıyı çeker — bu işlem makinenizin dışına çıkar.',
    noLinks: 'Bu mesajda bağlantı yok.',
    enablePrompt:
      'Mail servisi etkin değil. Uygulamanız gönderdikçe yakalanan mailler burada görünür — şimdi etkinleştirilsin mi?',
    enableAction: '{service} etkinleştir',
    startAction: '{service} başlat',
    enabling:
      'Etkinleştiriliyor — .env yazılıyor, yeniden üretiliyor ve konteyner başlatılıyor. İlk çalıştırma imajı indirir, bir dakika verin.',
    count: '{n} mesaj yakalandı',
    empty: 'Henüz hiçbir şey gönderilmedi.',
    noSubject: '(konu yok)',
    notRunning: 'Posta yakalayıcı çalışmıyor, hiçbir şey yakalanmıyor.',
    clear: 'Gelen kutusunu boşalt',
    release: 'Gönder',
    releaseTo: 'Bu mesajı şuraya ilet',
    releaseHint:
      'Gerçek bir adres ya da virgülle ayrılmış birkaç adres. Kopyası yakalayıcıda kalır.',
    released: 'Gönderildi.',
    relayTitle: 'Aktarım sunucusu',
    relayOff: 'Ayarlanmadı — gönderme reddedilir.',
    relayConfigure: 'Ayarla',
    relayExplain:
      'İletilen mesajın geçeceği SMTP sunucusu. Uygulamanızın gönderdiği hiçbir şey buraya gitmez — yakalayıcı hepsini yakalamaya devam eder, yalnızca sizin ilettiğiniz mesaj dışarı çıkar.',
    relayEnable: 'Mesaj iletmeye izin ver',
    relayHost: 'SMTP sunucusu',
    relayPort: 'Port',
    relaySecurity: 'Güvenlik',
    relayNoTls: 'Yok',
    relayUsername: 'Kullanıcı adı',
    relayPassword: 'Parola',
    relayPasswordSet: 'Parola (saklı — korumak için boş bırakın)',
    relayForget: 'Parolayı unut',
    relayFrom: 'Gönderen',
    relayFromHint: 'Sağlayıcılar sahibi olmadıkları bir gönderen adresini reddeder.',
    relayAllowed: 'Yalnızca şuraya gönderilebilsin',
    relayAllowedHint:
      'Virgülle ayırın. Boş bırakmak “her yere” demektir; bu da tek yazım hatası uzaklıktadır.',
    relayNoKeystore:
      'Bu makinede anahtar deposu yok, parola saklanamaz. Parola istemeyen bir sunucu kullanın.',
    relayRestart:
      'Yakalayıcı bunları yeniden oluşturulduğunda okur — kaydettikten sonra yığını yeniden başlatın.',
    deleteOne: 'Bu mesajı sil',
    confirmClear:
      'Yakalanan tüm mesajlar silinecek. Posta yakalayıcı bir çöp kutusudur, yedeği yoktur.',
  },
  db: {
    title: 'Yedekleme',
    subtitle: '{db} veritabanını dışa aktar ve geri yükle.',
    subtitleAll: 'Bu sunucudaki tüm veritabanlarını dışa aktar ve geri yükle.',
    notRunning: 'Konteyner çalışmıyor, okunacak bir şey yok.',
    dump: 'Yedekle',
    restore: 'Geri yükle',
    dumped: '{path} dosyasına yazıldı',
    restored: '{path} dosyasından geri yüklendi',
    confirmRestore:
      '{db} içeriği seçilen dosyanın içeriğiyle değiştirilecek. Şu anda içinde ne varsa kaybolur.',
  },
  snapshots: {
    title: 'Snapshot’lar',
    subtitle:
      'Bu uygulamanın sakladığı ve geri koyabildiği adlandırılmış bir kopya. Çalışma alanında durur, yani İndirilenler’de değil stack’le birlikte kalır.',
    name: 'Bu snapshot’a bir ad verin',
    take: 'Al',
    restore: 'Geri yükle',
    delete: 'Sil',
    none: 'Bu veritabanının henüz snapshot’ı yok.',
    automatic: 'zamanlanmış olarak alındı',
    restored: '{name} geri yüklendi',
  },
  xdebug: {
    title: 'Xdebug',
    subtitle: 'Bu proje için adım adım hata ayıklama.',
    on: 'Etkin',
    off: 'Devre dışı',
    firstTime:
      'İlk kez açmak uzantıyı imaja ekliyor ve yeniden derleme gerektiriyor. Ondan sonra aç/kapa yalnızca konteyneri yeniden başlatıyor — uzantı kalıyor ve kapalıyken hiçbir maliyeti yok.',
    staysInstalled:
      'Bu kapalıyken uzantı imajda kalıyor. Orada bir maliyeti yok, ve hata ayıklamayı yeniden açmak yeniden derleme değil bir konteyner yeniden başlatması.',
    needsRebuild:
      'Eklenti imaja derleniyor, bu yüzden proje yeniden üretilip derlenene kadar bunun bir etkisi olmaz.',
    notActive:
      'Çalışan konteyner Xdebug ayarlarını taşımıyor. Uygulanması için projeyi yeniden başlatın.',
    active: 'Çalışan konteynerde etkin — bir kesme noktası koyup siteyi açın.',
    ideSettings: 'IDE ayarları',
    port: 'Port',
    ideKey: 'IDE anahtarı',
    serverName: 'Sunucu adı (PHP_IDE_CONFIG)',
    pathMapping: 'Yol eşlemesi',
    version: 'Xdebug sürümü',
    cliCaveat:
      'Not: komut satırından `stackvo up` bu yapılandırmayı katmanlamaz ve konteyneri onsuz yeniden oluşturur.',
  },
  stackPreset: {
    export: 'Bu yığını dışa aktar',
    exportDesc:
      'Hangi servislerin etkin olduğunu ve sürümlerini küçük bir JSON dosyasına yazar; sürüm kontrolüne eklemek güvenlidir. Parolalar içinde yer almaz — biçimde onları koyacak bir yer yoktur.',
    name: 'Önayar adı',
    namePlaceholder: 'örn. ekip-backend',
    saveFile: 'Dosyaya kaydet…',
    summary: '{total} servisten {enabled} tanesi etkin.',
    preview: 'Dosyanın içeriği',
    import: 'Önayar içe aktar',
    importDesc:
      'Hiçbir şey yazılmadan önce tam olarak neyin değişeceğini gösterir. Parolalarınıza ve portlarınıza dokunulmaz.',
    chooseFile: 'Dosya seç…',
    untitled: 'Adsız önayar',
    colSubject: 'Ne',
    colFrom: 'Şimdi',
    colTo: 'Sonra',
    absent: 'tanımsız',
    apply: '{n} değişikliği uygula',
    applied: 'Uygulandı.',
    alreadyMatches: 'Bu yığın önayarla zaten uyuşuyor — {n} ayar denetlendi, hiçbiri farklı değil.',
    nothingUsable: 'Bu önayardaki hiçbir şey StackVo’nun bu sürümüne uygulanmıyor.',
    rejected: 'Uygulanmadı:',
    thenRegenerate:
      'Bir servisi etkinleştirmek üreticinin çıktısını değiştirir — yapılandırmayı yeniden üretip yığını ayağa kaldırın.',
  },

  dumps: {
    source: { web: 'Web', cli: 'CLI', queue: 'Kuyruk' },
    regex: 'Düzenli ifade',
    filterSource: 'Kaynağa göre süz',
    copy: 'Görünenleri kopyala',
    copyValue: 'Değeri kopyala',
    pause: 'Duraklat',
    resume: 'Sürdür',
    resumeHint: 'Sürdür — {n} yeni',
    clearHint: 'Listeyi ve kaydedilmiş olayları temizle',
    capturingCount: '{total} projeden {on} tanesi yakalıyor.',
    needsRecreateShort: 'Konteynerin yeniden oluşturulması gerekiyor',
    allDescription: 'Yakalaması açık her projeden gelen dump() ve dd() çıktıları',
    noProjects: 'Köprüyü kullanabilecek bir PHP projesi yok.',
    allProjects: 'Tüm projeler',
    capture: 'dump() ve dd() yakala',
    captureHint: 'Anında etkili — konteynere dokunulmaz.',
    help: 'Bu bölüm hakkında',
    captureOff: 'Yakalama kapalı. Açtığınızda dump() çıktıları burada birikir.',
    search: 'Ara',
    title: 'Dump’lar',
    explain:
      'dump() ve dd() çıktılarını yanıttan alıp burada gösterir. Biçimlendirmeyi, projenizin konteyneri içinde çalışan Symfony’nin kendi dump sunucusu yapar.',
    needsRecreate:
      'Çalışan konteynerde dump ayarları henüz yok. Bunlar konteyner oluşturulurken sabitlenir, o yüzden yeniden başlatmak yetmez — konteyneri yeniden oluşturmak gerekir.',
    clear: 'Temizle',
    waiting: 'Bir dump bekleniyor… uygulamada herhangi bir yerde dump() çağırın.',
    ddEndsTheRequest:
      'dump() isteği sürdürür. dd() ise dökümü alıp isteği bitirir ve Symfony bunu 500 olarak işaretler — tarayıcıda hata görürken dökümün burada belirmesi normaldir.',
  },

  release: {
    pushExplain:
      'Bir registry’ye gönderin ya da çalıştıracak bir compose dosyası alın. StackVo yalnız doğrulanmış bir imajı ve yalnız registry adı taşıyan bir etikete gönderir — registry katmanları saklar, sonradan etiketi silmek içindekini kaldırmaz.',
    pushCheck: 'Denetle',
    push: 'Gönder',
    recipe: 'Dağıtım reçetesi',
    load: 'Paket yükle',
    loadExplain:
      'Kaydet’in yazdığı bir .tar dosyasını bu makinenin Docker’ına geri okur. İnternete kapalı bir devrin alıcı ucu olduğu için ne proje ne plan gerektirir.',
    loaded: 'Docker şunları aldı:',
    title: 'Üretim imajı',
    explain:
      'Bu projenin hâlihazırda çalıştırdığı imajdan türetilen, dağıtılabilir bir imaj — aynı PHP sürümü, aynı eklentiler, aynı web sunucusu. Onun kopyası değil: geliştirme imajında uygulama kodu yok (kaynak diskinizden bağlanıyor) ve Xdebug taşıyor.',
    tag: 'İmaj etiketi',
    tagHint: '{base} üzerine kurulur',
    build: 'Derle',
    excluded: 'İmajın dışında tutulanlar',
    dockerfile: 'Kullanılacak Dockerfile',
    checked: 'Derlenen imaj gerçekte ne içeriyor',
    clean: '{tag} hazır. Dockerfile okunarak değil, imaj çalıştırılarak denetlendi.',
    notClean: 'Bu imaj henüz gönderilmeye uygun değil.',
    leaked: 'Ortam dosyaları imajın içinde: {files}',
    noEnv: 'Ortam dosyası yok — yapılandırmayı çalıştırırken verin.',
    xdebugOn: 'Xdebug hâlâ etkin. Bunu dağıtmayın.',
    xdebugOff: 'Xdebug etkin değil.',
    noApp: 'İmajda uygulama dosyası yok.',
    save: 'Tarball olarak kaydet…',
  },

  profiler: {
    title: 'Profilleyici',
    explain:
      'Xdebug’in kendi profilleyicisi; çıktıyı bu uygulamanın okuduğu dosyalara yazar. Hesap da ek eklenti de gerekmez — adım adım hata ayıklamayı yapan Xdebug’in ta kendisi.',
    needsXdebug: 'Önce Xdebug’i açın — profilleme aynı eklentinin bir kipidir.',
    modeDebug: 'Adım adım hata ayıklama',
    modeProfile: 'Profilleme',
    modeTrace: 'İz',
    traceCost:
      'İz kaydı her fonksiyon girişini ve çıkışını yazar; profilden çok daha ağırdır — tek bir istek yüzlerce megabayta çıkabilir. Bir sayfayı kaydedin, sonra geri alın.',
    traces: 'Kaydedilen izler ({n})',
    flameSummary: '{records} giriş/çıkış kaydı, {stacks} ayrı yığın, toplam {total} ms.',
    traceTruncated: 'İz, bu uygulamanın okuduğundan uzundu. Çizilen isteğin tamamı değil, başı.',
    tracePruned: '{n} yol çizilemeyecek kadar inceydi ve gösterilmiyor.',
    traceDepthCapped: 'Yığın 64 kareden derine indi; oranın altı ölçüldü ama çizilmedi.',
    modesExclusive:
      'Biri ya da diğeri. Adım ayıklama her istekte bağlanır, profilleme bir tetikleyici bekler; ikisini birden açık bırakmak birini bozar.',
    howToRecord:
      'Bir istek talep etmeden hiçbir şey kaydedilmez. URL’ye ?{trigger}=1 ekleyin veya çerez olarak tanımlayın.',
    modeMismatch: 'Konteyner şu an “{running}” modunda, ayar “{wanted}”.',
    needsRecreate:
      'Çalışan konteynerde bu henüz yok. Ortam değişkenleri ve bağlamalar konteyner oluşturulurken sabitlenir, o yüzden yeniden başlatmak yetmez — konteyneri yeniden oluşturmak gerekir.',
    recorded: 'Kayıtlı profiller ({n})',
    noneYet: 'Henüz kayıt yok.',
    clear: 'Tümünü sil ({size})',
    compressed: 'gzip’li',
    open: 'Aç',
    deleteOne: 'Bu profili sil',
    summary: '{n} fonksiyon · ölçülen işin {total} kadarı · {creator}',
    flame: 'Çağrı ağacı',
    flameHint: 'Neyin neyi çağırdığı ve her dalın maliyeti.',
    noTree:
      'Bu profil hiç çağrı kaydetmemiş — tek bir fonksiyon, ya da kuyruğu kesilmiş bir dosya.',
    truncated:
      'Bu profil okuma sınırından büyüktü; aşağıdaki sayılar yalnızca bir bölümünü kapsıyor.',
    colFunction: 'Fonksiyon',
    colSelf: 'Kendi süresi',
    colInclusive: 'Çağrılarla',
    colCalls: 'Çağrı',
  },

  quickCmd: {
    title: 'Komutlar',
    explain:
      'Bu projede sık çalıştırdığınız komutlar; terminal açıp konteyner adını hatırlamanıza gerek kalmadan. Yalnızca projenin dosyalarının izin verdiği komutlar sunulur.',
    because: '{file} dosyasından',
    declared: 'bu projeden',
    opensTerminal: 'terminal açar',
    needsRunning: 'Bunlar projenin konteynerinin içinde çalışır. Önce projeyi başlatın.',
    none: 'Burada artisan, composer.json, package.json veya wp-config.php yok; sunulacak bir şey de yok.',
  },

  devServer: {
    title: 'Geliştirme sunucusu',
    explain:
      'İmaja gömülü üretim derlemesi yerine, kaynağınız canlı bağlanmış hâlde projenin geliştirme sunucusunu çalıştırır. Bu olmadan konteyner, derlendiği anda alınmış bir kod kopyası taşır; dosya düzenlemek hiçbir şeyi değiştirmez.',
    on: 'Açık — kaynak bağlı, geliştirme sunucusu çalışıyor',
    off: 'Kapalı — imajdaki üretim derlemesi',
    command: 'Geliştirme komutu',
    commandHint: 'Üretim komutunun yerine geçer; o komut: {production}',
    live: 'Canlı. Bir dosyayı kaydedin, tarayıcı takip etsin.',
    needsRecreate:
      'Geliştirme kipi açık ama çalışan konteyner kaynak bağlaması olmadan oluşturulmuş. Projeyi yeniden ayağa kaldırın.',
    projectConfig: 'Projenizin de şuna ihtiyacı var',
    projectConfigWhy:
      'Bu kısım sizin deponuzda yaşıyor, bu yüzden yazılmıyor yalnızca gösteriliyor. Vite, yapılandırmasının tanımadığı bir alan adına 403 döner; sıcak yenileme istemcisine de tarayıcının gerçekte hangi portta olduğu söylenmelidir — proxy’nin arkasında bu 443’tür, geliştirme sunucusunun kendi portu değil.',
    notAllowed: '{file} bundan söz etmiyor — bu alan adına gelen istekler 403 dönecek.',
    configured: 'Yapılandırmanız bunu zaten karşılıyor.',
    noAdvice:
      'package.json içinde Vite, Nuxt veya Next bulunamadı; verilecek yapılandırma önerisi yok — kaynak bağlaması yine de geçerli.',
    modulesNote:
      'node_modules kendi biriminde kalır, böylece bağlama imajın Linux için yaptığı kurulumu gizlemez. Bağımlılıkları değiştirdikten sonra projeyi yeniden derleyin.',
    cliCaveat:
      'Not: komut satırından `stackvo up` bunu katmanlamaz ve konteyneri üretim kipinde yeniden oluşturur.',
  },

  phpIni: {
    title: 'PHP ayarları',
    explain:
      'Bu projeye özel değerler; .stackvo/php.ini dosyasına yazılır ve PHP’nin conf.d dizinine salt okunur bağlanır. PHP kendi php.ini dosyasından sonra okuduğu için burada yazan geçerli olur. Elle düzenlemek de sürüm kontrolüne eklemek de güvenlidir.',
    field: {
      memory_limit: 'Bellek sınırı',
      upload_max_filesize: 'En büyük yükleme boyutu',
      post_max_size: 'En büyük POST boyutu',
      max_execution_time: 'En uzun çalışma süresi',
    },
    notMeasured: 'tanımsız',
    measured: 'Alan içindeki değerler, çalışan konteynerdeki PHP’nin şu anki değerleridir.',
    hint: {
      memory_limit: 'K, M veya G ekli bir sayı. Sınırsız için -1.',
      upload_max_filesize: 'İkisinden küçüğü geçerli olduğu için POST boyutu bunu kısıtlar.',
      post_max_size: 'En az yükleme boyutu kadar olmalı.',
      max_execution_time: 'Tam saniye. Sınırsız için 0.',
    },
    save: 'Kaydet',
    removeFile: 'Dosyayı kaldır',
    emptyRemoves: 'Boş bırakılan alan yönergeyi kaldırır.',
    needsRestart:
      'Kaydedildi. PHP yapılandırmasını başlangıçta okur — uygulanması için projeyi yeniden başlatın.',
    needsRecreate:
      'Dosya diskte ama çalışan konteynerde bağlaması yok. Eklenmesi için projeyi yeniden ayağa kaldırın.',
    unmanaged: 'Bu dosyadaki diğer yönergeler',
    file: 'Dosya',
    mountedAt: 'Bağlandığı yer',
    cliCaveat:
      'Not: komut satırından `stackvo up` bu bağlamayı katmanlamaz ve konteyneri onsuz yeniden oluşturur.',
  },
  certs: {
    title: 'HTTPS sertifikası',
    subtitle: 'Tek bir joker sertifika panoyu, her servisi ve her projeyi kapsar.',
    sslOff:
      '.env içinde SSL_ENABLE kapalı, yani yığın HTTP üzerinden sunuluyor ve sertifika kullanılmıyor.',
    current: 'Güncel',
    stale: 'Yeniden üretilmeli',
    caTrusted: 'CA güveniliyor',
    caUntrusted: 'CA güvenilmiyor',
    caUnknown: 'CA güveni bilinmiyor',
    expiresOn: 'Bitiş {date} ({days} gün)',
    expiredOn: '{date} tarihinde doldu',
    noMkcert: 'mkcert kurulu değil, bu yüzden sertifika üretilemez ya da yenilenemez.',
    missing: 'Kapsam dışı — bu alan adları tarayıcı uyarısı verecek',
    dropping: 'Sonraki yenilemede kapsam dışı kalacak',
    rejected: 'Atlandı — geçerli alan adı değil',
    covered: 'Kapsanan ({n})',
    reissue: 'Sertifikayı yenile',
    trustInTerminal: 'CA’ya güven (terminalde)',
    trustInTerminalHint:
      'macOS, güven ayarlarını yalnızca etkileşimli olarak değiştirtiyor — pencereli bir uygulama bunu kendi başına yapamıyor. Düğme terminalinizi açıp `sudo parolanızı sorar. Sonra tarayıcıyı tamamen kapatıp açın.',
    leafLabel: 'Sertifika',
    caLabel: 'İmzalayan CA',
    whySeparate:
      'İkisi ayrı dizinde çünkü sertifika dizini Traefik konteynerine bağlanıyor. CA’nın özel anahtarı oraya konsaydı, o konteynerdeki herhangi bir süreç bu makinenin güvendiği her alan adı için sertifika üretebilirdi. CA ayrıca yeniden üretilmez — silinirse tarayıcıya verdiğiniz güven de gider.',
    notReloaded:
      'Sertifika yenilendi, ancak proxy hâlâ öncekini sunuyor. Devreye girmesi için stack’i yeniden başlatın veya generate çalıştırın.',
  },
  serviceCategories: {
    databases: 'Veritabanları',
    cache: 'Önbellek',
    queue: 'Kuyruklar',
    search: 'Arama',
    storage: 'Nesne depolama',
    monitoring: 'İzleme',
    devtools: 'Geliştirici araçları',
    adminUis: 'Yönetim arayüzleri',
  },
  instanceSettings: {
    fields: {
      VERSION: 'Sürüm',
      URL: 'Alt alan adı',
      HOST_PORT: 'Ana makine portu',
      PORT: 'Port',
      HOST: 'Sunucu',
      DATABASE: 'Veritabanı',
      DB: 'Veritabanı',
      USER: 'Kullanıcı adı',
      PASSWORD: 'Parola',
      ROOT_PASSWORD: 'Root parolası',
      ADMIN_USER: 'Yönetici kullanıcı adı',
      ADMIN_USERNAME: 'Yönetici kullanıcı adı',
      ADMIN_PASSWORD: 'Yönetici parolası',
      ADMIN_PASS: 'Yönetici parolası',
      DEFAULT_USER: 'Varsayılan kullanıcı',
      DEFAULT_PASS: 'Varsayılan parola',
      DEFAULT_PASSWORD: 'Varsayılan parola',
      DEFAULT_EMAIL: 'Varsayılan e-posta',
      BASICAUTH_USERNAME: 'Basic auth kullanıcı adı',
      BASICAUTH_PASSWORD: 'Basic auth parolası',
      INITDB_ROOT_USERNAME: 'İlk kurulum root kullanıcısı',
      INITDB_ROOT_PASSWORD: 'İlk kurulum root parolası',
      UPLOAD_LIMIT: 'Yükleme sınırı',
      CLUSTER_NAME: 'Küme adı',
      ROOT_USER: 'Root kullanıcı adı',
      REGION: 'Bölge',
      MASTER_KEY: 'Ana anahtar',
      API_KEY: 'API anahtarı',
      CONSOLE_HOST_PORT: 'Konsol host portu',
    },
    none: 'Bu paketin ayarlanacak bir şeyi yok.',
    default: 'varsayılan',
    reveal: 'Göster',
    hide: 'Gizle',
    showKey: 'Ayar anahtarını göster ({key})',
    requiredMissing: 'Zorunlu ve boş: {keys}',
    firstBootWarning:
      'Bu instance’ın verisi zaten varsa {keys} etkili olmayabilir: MySQL ve Postgres gibi imajlar kimlik bilgilerini yalnızca boş bir veri dizinini ilk kurarken okur. Konteyner her hâlükârda yeniden oluşturulur — veritabanının içindeki değer değişmez. Servisin kendi araçlarıyla değiştirin, ya da instance’ı ve volume’ünü silip yeniden oluşturun.',
    reset: 'Paketin varsayılanına dön ({value})',
    secretChanged: 'değiştirildi',
    discardTitle: 'Değişiklikler atılsın mı?',
    discardBody: 'Girdiğiniz değerler uygulanmadı ve kaybolacak.',
    ports: 'Host portları',
    portsSubtitle:
      'Bu instance’ın makinenizde yayınladığı numara. Boş olup olmadığı uygularken denetlenir — hem bu makineye hem de diğer tüm instance’lara karşı.',
    portOf: '{handle} portu',
    apply: 'Uygula ve yeniden kur',
    confirmTitle: 'Konteyner yeniden kurulsun mu?',
    confirmBody:
      'Bunları kaydetmek tek başına yetmez: {instance} kurulduğu ortamla çalışıyor, bu yüzden konteyneri durdurulup yeni değerlerle yeniden oluşturulacak.',
    confirmApply: 'Uygula',
  },
  about: {
    tagline: 'Yerel geliştirme ortamları, tek bir stack olarak yönetilir.',
    system: 'Sistem bilgisi',
    systemDesc: 'Bir hata bildiriminin ihtiyaç duyduğu bilgiler. Yeniden yazmak yerine kopyala.',
    appVersion: 'StackVo',
    os: 'İşletim sistemi',
    docker: 'Docker',
    context: 'Docker bağlamı',
    workspace: 'Çalışma alanı',
    copy: 'Kopyala',
    copied: 'Kopyalandı',
    resources: 'Kaynaklar',
    resourcesDesc: 'Tarayıcında açılır.',
    links: {
      docs: 'Belgeler',
      source: 'Kaynak kodu',
      issues: 'Sorun bildir',
      sponsor: 'Bir kahve ısmarla',
    },
    copyright: 'MIT lisanslı · © 2026 Fahrettin Aksoy',
    licences: 'Üçüncü taraf lisansları',
    licencesDesc: 'Bu sürümle birlikte gelen bildirimler, derlendiği hâliyle.',
    licencesFailed: 'Lisans bildirimi bu sürümden okunamadı.',
    close: 'Kapat',
  },
  settings: {
    servers: {
      gzipTypesHint:
        'Boşlukla ayrılmış MIME türleri. Boş bırakılırsa nginx’in kendi listesi kalır.',
      field: {
        SERVER_MAX_BODY_SIZE: 'Azami gövde boyutu',
        SERVER_CLIENT_BODY_TIMEOUT: 'İstemci gövde zaman aşımı',
        SERVER_KEEPALIVE_TIMEOUT: 'KeepAlive zaman aşımı',
        SERVER_FASTCGI_CONNECT_TIMEOUT: 'FastCGI bağlanma zaman aşımı',
        SERVER_FASTCGI_SEND_TIMEOUT: 'FastCGI gönderme zaman aşımı',
        SERVER_FASTCGI_TIMEOUT: 'FastCGI okuma zaman aşımı',
        SERVER_TCP_NODELAY: 'TCP nodelay',
        SERVER_GZIP: 'Gzip',
        SERVER_GZIP_COMP_LEVEL: 'Gzip seviyesi',
        SERVER_GZIP_TYPES: 'Gzip türleri',
      },
      extra: 'Ek yönergeler',
      extraDesc:
        'Bu sunucu için üretilen her yapılandırmaya eklenir. Yorumlar ve boş satırlar atılır, yani yalnızca not içeren bir dosya hiçbir şeyi değiştirmez.',
      extraPlaceholder: 'client_body_timeout 120s;',
      extraHint: "{'{{ VAR }}'} .env üzerinden yerine konur. Bir sonraki üretimde etkili olur.",
      title: 'Web sunucuları',
      desc: 'PHP’nin önündeki sunucunun neyi kabul edeceği.',
      limits: 'İstek sınırları',
      limitsDesc:
        'Üretilen sunucu yapılandırmasına yazılır. Varsayılanda bırakılırsa hiçbir şey yazılmaz.',
      sizeInvalid: 'Sayı, ardından isteğe bağlı k, m ya da g.',
      secondsInvalid: 'Tam saniye.',
      phpNote:
        'Bir yükleme, sınırların en düşüğünde reddedilir. PHP’nin kendi sınırları vardır — upload_max_filesize, post_max_size ve memory_limit — ve onlar proje başınadır, projenin PHP ayarlarında.',
      applies: 'Nerede geçerli',
      appliesDesc: 'Her sunucu bir dosya üzerinden yapılandırılmıyor.',
      supportNote:
        'Apache kendi Dockerfile’ı içinde, Swoole ise satır içi bir betikle yapılandırılıyor; ikisinin de yönerge eklenecek bir dosyası yok. Yukarıdaki istek sınırları yalnızca nginx ve caddy’ye yazılır — FrankenPHP’nin Caddyfile’ı onları taşımıyor, ona yalnızca ek yönerge yazabilirsiniz.',
    },
    defaults: {
      title: 'Proje varsayılanları',
      desc: 'Hangi çalışma ortamı olursa olsun, yeni bir projenin başlangıç değerleri.',
      runtimes: 'Çalışma sürümleri',
      php: 'PHP ve web sunucusu',
      phpTools: 'PHP derlemesi',
    },
    workspaceAndControl: 'Dizin ve kontrol',
    workspaceAndControlDesc:
      'Bu stack’in nerede durduğu, nasıl çalıştırıldığı ve nasıl paylaşıldığı.',
    groups: {
      app: 'Uygulama',
      workspace: 'Çalışma alanı',
      stack: 'Stack',
      help: 'Yardım',
    },
    subtitle: 'Uygulama tercihleri',

    // Görünüm bölümü.
    appearance: 'Görünüm',
    appearanceSectionDesc: 'Tema, ana renk, nötr palet ve köşe yuvarlaklığını özelleştir.',
    themeColors: 'Tema ve renkler',
    themeColorsDesc: 'Uygulamanın görünümünü kişiselleştir',
    primaryColor: 'Ana renk',
    neutralPalette: 'Nötr palet',
    radius: 'Köşe yuvarlaklığı ({px}px)',
    resetAppearance: 'Varsayılanlar',
    typography: 'Tipografi ve okunabilirlik',
    typographyDesc: 'Yazı tipi, arayüz ölçeği ve kontrast',
    fontFamily: 'Yazı tipi',
    fontFamilyHint: 'Yalnızca sistemde kurulu yazı tipleri listelenir.',
    uiScale: 'Arayüz ölçeği ({px}px)',
    highContrast: 'Yüksek kontrast',
    highContrastHint: 'İkincil metni ve ayraçları belirginleştirir.',
    reduceMotion: 'Animasyonları azalt',
    density: 'Arayüz yoğunluğu',
    densityCompact: 'Sık',
    densityComfortable: 'Rahat',
    densitySpacious: 'Geniş',
    systemAccent: 'Sistem rengi',
    reduceMotionHint: 'Geçişleri kapatır; ilerleme göstergeleri dönmeye devam eder.',
    statusColors: 'Durum renkleri',
    statusColorsDesc: 'Çalışıyor, durdu ve hata hangi renkle anlatılsın',
    statusPalette: 'Palet',
    statusPalettes: {
      default: 'Varsayılan (yeşil / kırmızı)',
      colorblind: 'Renk körlüğü güvenli (Okabe-Ito)',
      muted: 'Yumuşak',
    },
    darkConsoles: 'Konsolları her zaman koyu tut',
    darkConsolesHint: 'Log ve terminal panelleri açık temada da koyu kalır.',
    presets: 'Ön ayarlar',
    presetsDesc: 'Bir görünümü adlandırıp sonra tek tıkla geri dön',
    presetName: 'Ön ayar adı',
    savePreset: 'Kaydet',
    noPresets: 'Henüz kayıtlı ön ayar yok.',
    neutrals: {
      graphite: 'Grafit',
      carbon: 'Karbon',
      midnight: 'Gece mavisi',
      forest: 'Orman',
      warm: 'Sıcak gri',
    },
    fonts: {
      system: 'Sistem',
      grotesk: 'Grotesk (Helvetica)',
      serif: 'Serif (Georgia)',
      mono: 'Monospace',
    },

    // Yerelleştirme bölümü.
    localisation: 'Yerelleştirme',
    localisationDesc: 'Arayüz dili ve yazım yönü.',
    languageDesc: 'Arayüz ve tepsi menüsünün dili',
    consoleLanguage: 'Konsol dili',
    consoleLanguageDesc: 'Log ve terminal panellerinin dili',
    consoleLanguageHint: 'Hata çıktısını paylaşırken arayüz dilinden bağımsız tutmak için.',
    consoleFollowsApp: 'Arayüzle aynı',
    direction: 'Yazım yönü',
    directionDesc: 'Arayüzün hangi yönde aktığı',
    rtl: 'Sağdan sola düzen',
    rtlHint: 'Tüm bileşenler aynalanır; Arapça ve İbranice yerleşimleri denemek için.',

    // Bölüm açıklamaları: her panelin ne işe yaradığı, panele girerken bir kez.
    preferencesDesc: 'Görünüm, dil, dış uygulamalar ve kapatma davranışı.',
    certificates: 'Sertifikalar',
    certificatesDesc: 'HTTPS sertifikası, kapsadığı alan adları ve arkasındaki CA.',
    aboutDesc: 'Sürüm, imzalı güncellemeler ve tanılama.',

    // Alt gruplar.
    workspaceGroup: 'Çalışma dizini',
    workspaceGroupDesc: 'Bu uygulamanın yönettiği checkout',

    templates: {
      title: 'Şablon geçersiz kılmaları',
      description:
        'Şablonlar uygulamanın içinde gömülü. Bir dosya yalnızca siz devraldığınızda çalışma dizininde belirir — ve o andan itibaren güncellemeler ona uğramaz.',
      count: '{total} şablonun {count} tanesi bu çalışma dizininde devralınmış.',
      none: '{total} şablonun tamamı gömülü sürümden okunuyor.',
      pick: 'Devralınacak şablon',
      pickHint: 'Dosya çalışma dizinine kopyalanır ve editörünüzde açılır.',
      override: 'Devral ve düzenle',
      open: 'Aç',
      revert: 'Varsayılana dön',
      revertTitle: 'Devralınan şablon silinsin mi?',
      revertBody:
        'Sizin düzenlediğiniz dosya silinir ve gömülü sürüm devreye girer. Düzenlemenizin başka bir kopyası yok — bu işlem geri alınamaz.',
      reload: 'Yenile',
    },
    engineGroupDesc: 'Konteynerleri çalıştıran motorun durumu',
    externalApps: 'Dış uygulamalar',
    externalAppsDesc: 'Terminal ve editör hangi uygulamada açılsın',
    backups: 'Otomatik yedekler',
    backupsDesc: 'Zamanlanmış olarak alınan, çalışma alanında tutulan snapshot’lar.',
    backupSchedule: 'Snapshot alma sıklığı',
    backupScheduleHint:
      'Saatten değil, son snapshot’tan ölçülür — üç gün kapalı kalmış bir dizüstü üç değil bir snapshot borçludur. Yalnızca çalışan veritabanları yedeklenir.',
    backupOff: 'Hiçbir zaman',
    backupHourly: 'Saatte bir',
    backupDaily: 'Günde bir',
    backupWeekly: 'Haftada bir',
    backupKeep: 'Saklanacak zamanlanmış snapshot sayısı',
    backupKeepHint:
      'Bu sayının ötesindeki en eski zamanlanmışlar silinir. Kendi adlandırdığınız snapshot’lar asla silinmez ve bu sayıya dahil edilmez.',
    startup: 'Başlangıç ve kapatma',
    startupDesc: 'Uygulama açılırken ve kapanırken ne olsun',
    compose: 'Konteynerler',
    generatorDesc: 'Diskteki dosyaları üreticinin yazacağıyla karşılaştırır',
    updatesDesc: 'İmzalı sürüm denetimi ve kurulum',

    theme: 'Tema',
    language: 'Dil',
    packProgress: '{total} dizeden {done} tanesi ({percent}%) — kalanı İngilizce görünür',
    packRemove: 'Kaldır',
    packTag: 'Dil etiketi',
    packHint:
      'de, fr ya da pt-BR gibi bir etiket. Çevirebileceğiniz bir dosya oluşturur; çevrilmeyen dizeler İngilizce kalır.',
    packStart: 'Çeviri başlat',
    preferences: 'Tercihler',
    stackSub: 'Compose seviyesinde: yeniden üretir ve konteynerleri yeniden kurar.',
    runtimes: {
      desc: 'Her çalışma ortamında yeni bir projenin başlayacağı sürüm. Hangi sürümlerin var olduğu uygulamanın kendi kataloğudur, ayar değildir.',
    },
    php: {
      versionDesc:
        'Yeni bir PHP projesinin başlangıç değerleri. Var olan projeler kendi stackvo.json dosyasındaki sürümü korur.',
      version: 'PHP sürümü',
      versionHint:
        'Yeni proje formunda önceden seçili gelir; her proje yine kendi sürümünü seçebilir.',
      server: 'Web sunucusu',
      serverHint:
        'PHP projelerini sunar. Diğer çalışma zamanları kendi geliştirme sunucusunu çalıştırır.',
      composer: 'Composer sürümü',
      composerHint: 'PHP imajına kurulur. "latest" derleme anındaki güncel sürümü izler.',
      nodejs: 'Node.js sürümü',
      nodejsHint:
        'PHP konteyneri içindeki varlık derlemeleri için — Node projesi çalışma zamanından ayrıdır.',
    },
    secrets: {
      title: 'Kimlik bilgileri nerede tutuluyor',
      description:
        'Veritabanı şifreleri, token’lar ve sunucu kimlikleri .env yerine bu makinenin anahtar deposunda durabilir.',
      whatItDoes:
        'Bir kimlik bilgisini taşımak, onu Keychain, Credential Manager veya Secret Service içine kaydeder ve .env’de bir referans bırakır. Değer artık yedeklenen, senkronlanan ve destek konularına yapıştırılan dosyada değil.',
      stillGenerated:
        'Değer hâlâ generated/docker-compose.dynamic.yml içine yazılıyor — Compose onu oradan okuyor. Bu işlem şifreyi .env’den çıkarır; diskten çıkarmaz.',
      cliCannotRead:
        'stackvo.sh komut satırı aracı bunları okuyamaz. Bu çalışma alanında onu da kullanıyorsanız kimlik bilgilerini .env’de bırakın.',
      noKeystore:
        'Bu makinede uygulamanın ulaşabildiği bir anahtar deposu yok, bu yüzden hiçbir şey taşınamaz.',
      unresolvable:
        'Bu kimlik bilgileri anahtar deposunu işaret ediyor ama depo cevap vermedi. Çözülene kadar dosya üretimi engelli — anahtar zincirinizi açın ya da değeri geri alın.',
      none: 'Bu çalışma alanında tanımlı kimlik bilgisi yok.',
      inKeystore: 'Anahtar deposunda',
      inEnvFile: '.env içinde, düz metin',
      move: 'Taşı',
      restore: 'Geri al',
    },
    localApi: {
      title: 'Yerel API',
      sectionDesc: 'Bu makinede salt-okunur bir HTTP yüzeyi',
      description:
        'Bu çalışma alanı hakkındaki soruları HTTP üzerinden, bu makinedeki token’a sahip her şeye cevaplar.',
      whatItDoes:
        'MCP sunucusunun kullandığı araç tablosunun salt-okunur yarısını servis eder; yalnız 127.0.0.1 üzerinde, başka hiçbir yerde. Buradaki hiçbir şey yazmaz, komut çalıştırmaz, parola göstermez.',
      readsOnly:
        'Siz başlatana kadar kapalı. Kimsenin haberi olmayan bir dinleyici, kimsenin kapatmadığı dinleyicidir.',
      start: 'Başlat',
      stop: 'Durdur',
      notRunning: 'Çalışmıyor',
      tokenShownOnce:
        'Bu token bir kez gösterilir. Diske hiç yazılmaz — kaybederseniz durdurup yeniden başlatın.',
      tokenGone:
        'Çalışıyor, ama token daha önceki bir oturuma gösterildi. Yenisi için durdurup yeniden başlatın.',
      tokenPlaceholder: '<token>',
      example: 'Deneyin',
      served: '{count} araç servis ediliyor',
    },
    agents: {
      title: 'Yapay zekâ asistanları',
      sectionDesc: 'StackVo MCP sunucusunu bu makinedeki asistanlara tanıtın.',
      description:
        'Bu sunucuya sahip bir asistan, “shop.loc neden açılmıyor?” sorusunu ön kontrol raporundan, hosts dosyasından, sertifikadan ve bir container’ın loglarından cevaplayabilir.',
      whatItDoes:
        'Ekleme, o uygulamanın kendi yapılandırma dosyasına stackvo adında tek bir girdi yazar. Dosya her satırda yazılı, kendiniz açabilesiniz diye.',
      neverClobbers:
        'Dosyadaki başka hiçbir şeye dokunulmaz ve yazmadan önce yanına .stackvo-backup adıyla bir kopya bırakılır.',
      noBinary:
        'stackvo-mcp ayrı bir binary ve uygulamayla birlikte gelmiyor. Tanıtılabilmesi için önce derlenmesi gerekiyor — aksi hâlde asistan var olmayan bir yolu işaret ederdi.',
      buildCommand: 'cargo build --release --bin stackvo-mcp',
      serverBinary: 'Tanıtılacak sunucu',
      allowWrites: 'Asistan değişiklik yapabilsin',
      allowWritesDetail:
        'Kapalıyken asistan yalnızca okuyabilir. Açıkken stack_up, stack_down, project_start, project_stop, generate, xdebug_set ve certificates_reissue de eklenir — yani stack’in tamamını durdurmak dahil. Bu ayar, eklediğiniz bir sonraki asistan için geçerlidir.',
      state: {
        registered: 'Tanıtıldı',
        stale: 'Tanıtıldı, ama başka bir kopyayı işaret ediyor',
        available: 'Kurulu, tanıtılmadı',
        absent: 'Bu makinede bulunamadı',
        unparseable: 'Bu dosya yorum satırı içeriyor, güvenle düzenlenemez',
      },
      add: 'Tanıt',
      update: 'Güncelle',
      remove: 'Kaldır',
      copyBlock: 'Bloğu kopyala',
      notListed:
        'Codex ve Zed listede yok: Codex yapılandırmasını TOML’da tutuyor, Zed’in biçimi ise doğrulanamadı. İkisi de yukarıdaki blokla elle yapılandırılabilir.',
    },
    policy: {
      title: 'Bu makine yönetiliyor',
      body: 'Bu makinedeki bir politika dosyası {count} ayarı belirliyor. Kilitlediği değerler burada değiştirilemez.',
      source: 'Politika dosyası:',
      registry: 'İmajlar şunun üzerinden çekiliyor:',
      notASecurityBoundary:
        'Politika dosyası, bu uygulamaya kurumunuzun niyetini bildirir. Bir güvenlik sınırı değildir — STACKVO_POLICY_FILE ile başka bir dosyaya yönlendirilebilir.',
      brokenTitle: 'Politika dosyası tam olarak uygulanmadı',
      brokenBody:
        'Aşağıdaki kısımlardan hiçbiri uygulanmadı ve uygulamanın geri kalanı yönetilmiyormuş gibi çalışıyor. Bu dosyayı dağıtan kişi muhtemelen yürürlükte olduğunu sanıyor.',
      managed: 'Yönetiliyor',
      managedHint: 'Bu değer, bu makinedeki bir politika dosyasından geliyor.',
      locked: 'Kilitli',
      lockedHint:
        'Bir politika dosyası bu değeri belirliyor ve burada değiştirilmesine izin vermiyor.',
    },
    shape: {
      title: 'Alan adı ve ağ',
      sectionDesc: 'Projelere nereden erişileceği ve nasıl sunulacağı.',
      suffixRequired: 'Sonek zorunlu; yönlendirmeler bundan kuruluyor.',
      suffixInvalid: 'Yalnızca harf, rakam, nokta ve tire; başı ve sonu harf veya rakam olmalı.',
      network: 'Docker ağı',
      networkHint:
        'Tüm servislerin katıldığı ağ. Adını değiştirmek sonraki başlatmada konteynerleri yeniden kurar.',
      networkRequired: 'Ağ adı zorunlu.',
      networkInvalid: 'Yalnızca harf, rakam, nokta, tire ve alt çizgi.',
      reset: 'Varsayılana dön',
      addressTitle: 'Adresler',
      addressDesc:
        'Projelerin ve servislerin yanıt verdiği yer. Her ana bilgisayar adı bu soneğin altında toplanır; tek bir sertifikanın hepsini kapsamasını sağlayan da budur.',
      suffixLabel: 'Ad alanı',
      suffixLabelHint:
        'Tüm adresleri tek bir üst alan altında toplar. İsteğe bağlı — boş bırakırsan yalnızca uzantı kullanılır.',
      suffixTld: 'Uzantı',
      suffixTldHint:
        '.test ve .localhost yerel kullanım için ayrılmıştır. .dev gerçek bir TLD’dir ve HTTPS ister.',
      preview: 'Adresler şöyle olur:',
      suffixHsts:
        'Bu uzantı tarayıcıların HSTS ön yükleme listesinde: altındaki hiçbir adres düz HTTP ile açılmaz ve uyarıyı geçme imkânı yoktur. Kullanmadan önce aşağıdan HTTPS’i aç.',
      networkTitle: 'Ağ ve TLS',
      networkGroupDesc: 'Servislerin paylaştığı Docker ağı ve HTTPS ile sunulup sunulmadıkları.',
      thenRegenerate:
        'Kaydedildi. Yönlendirme etiketlerinin bunu alması için yeniden üret — o ana kadar stack eski etiketlerle yanıt verir.',
      thenCertificates:
        'Yeni sonek kendi sertifikasını ister; ardından Sertifikalar bölümüne bak. Var olan projeler kendi stackvo.json dosyasındaki alan adını korur.',
      regenerate: 'Yeniden üret',
      ssl: 'HTTPS ile sun',
      sslHint: 'Yukarıdaki alan adı soneki için yerel sertifika üretir ve bağlar.',
      sslOffBreaksRouting:
        'HTTPS kapalıyken HTTPS giriş noktası üretilmiyor, ama bütün yönlendirmeler yine onu hedefliyor — yeniden açılana kadar hiçbir proje veya servis alan adı çözülmez.',
      proxyTitle: 'Ters proxy',
      proxyDesc:
        'Traefik. Her projeye ve yönetim arayüzüne onun üzerinden erişilir, TLS’i de o sonlandırır — yukarıdaki HTTPS anahtarının açtığı şey budur.',
      proxyPorts: 'Yayınlanan portlar',
      proxyDashboard: 'Panoyu aç',
      hostsTitle: 'Hosts dosyası',
      hostsDesc:
        'Buradaki her alan adı isimle çözülüyor, yani her biri /etc/hosts’ta bir satır ister. Değiştirmek parolanı sorar.',
      hostsFix: 'Tümünü düzelt',
      hostsOk: 'Hepsi çözülüyor',
      hostsManual: 'elle eklenmiş',
      hostsStale: 'StackVo’nun yazdığı ama artık gerekmeyenler — aynı düğme kaldırır:',
      redirect: 'HTTP’yi HTTPS’e yönlendir',
      redirectHint: 'Düz istekler sitenin kendisi yerine yönlendirmeyle yanıtlanır.',
      redirectBlocked: 'HTTPS açık olmalı — kapalı bir şemaya yönlendirmek hiçbir yere çıkmaz.',
      phpDesc:
        'Yeni bir PHP konteynerinin neyle kurulacağı. Değişiklik bundan sonra üretilen projeleri etkiler.',
      tools: 'Araçlar',
      toolsHint: 'PHP ile birlikte kurulur. Eklemek için yaz, kaldırmak için çarpıya tıkla.',
      apt: 'Sistem paketleri',
      aptHint: 'Konteyner içinde apt ile kurulur.',
    },
    about: 'Hakkında',
    diagnostics: 'Uygulama günlüğü',
    diagnosticsHint:
      'StackVo’nun kendi tanılama kaydı — projelerin sunucu logları değil. Bir sorun bildirirken bu klasörü ekleyin.',
    openLogs: 'Klasörü aç',
    logsUnavailable: 'Bu sistemde yazılabilir bir log konumu bulunamadı.',
    logsRedacted: 'Parola ve token değerleri log yazılırken maskelenir.',
    saveBundle: 'Tanılama paketi kaydet',
    saveBundleHint:
      'Log, başlangıç kontrolleri, doktor raporu ve varsa çökme raporları tek bir arşivde — bir hata bildirimi için gereken her şey, yalnızca log yerine.',
    saveBundleDone: 'Kaydedildi ({bytes}). İçi düz metindir; göndermeden önce bir bakın.',
    verifyNow: 'Üreteci şimdi doğrula',
    checkForUpdates: 'Güncellemeleri denetle',
    updates: 'Güncellemeler',
    version: 'Sürüm',
    upToDate: 'Güncel.',
    updateAvailable: 'Sürüm {version} hazır.',
    installUpdate: 'Kur ve yeniden başlat',
    updaterUnconfigured:
      'Bu yapı güncellemeleri doğrulayamaz: içine gömülü bir açık anahtar yok. Yayın imzalama anahtarı tanımlanana kadar güncelleme denetimi kapalı.',
    updateSigned: 'Paket imzası, uygulamaya gömülü anahtarla doğrulanır.',
    generator: 'Üretici (sapma denetimi)',
    generatorReady: 'disk, üreticinin yazacağıyla aynı',
    generatorDiffers: 'sapma var — üretilmiş bir dosya elle değişmiş ya da bayat',
    themeSystem: 'Sistem',
    themeLight: 'Açık',
    themeDark: 'Koyu',
    terminalApp: 'Terminal',
    editorApp: 'Kod editörü',
    browserApp: 'Tarayıcı',
    browserAppHint:
      '“Ziyaret et” düğmelerinin tamamı bunu kullanır — proje ve servis alan adları burada açılır.',
    appsHint: 'Kurulu olmayanlar seçilemez.',
    appDefault: 'Varsayılan',
    startMinimized: 'Tepsiye küçültülmüş başlat',
    autostart: 'Açılışta başlat',
    save: '{count} değişikliği kaydet',
    saved: 'Kaydedildi',
  },

  a11y: {
    copy: 'Panoya kopyala',
    moreActions: 'Diğer işlemler',
    followOutput: 'Çıktıyı takip et',
    stopFollowing: 'Çıktıyı takip etme',
    toggleConsole: 'Konsolu aç/kapat',
    loading: 'Yükleniyor',
    close: 'Kapat',
    primaryNav: 'Ana gezinme',
  },
  actions: {
    start: 'Konteyneri başlat',
    stop: 'Konteyneri durdur',
    restart: 'Konteyneri yeniden başlat',
    build: 'Projeyi derle',
    rebuild: 'Projeyi yeniden derle',
    generate: 'Yapılandırmayı üret',
    up: 'Yığını ayağa kaldır',
    down: 'Yığını durdur',
    composeRestart: 'Yığını yeniden başlat',
  },

  requirements: {
    title: 'Bu projenin ihtiyaç duyduğu servisler',
    description:
      'Ortam tanımının repoyla birlikte gelen yarısı: bir iş arkadaşı klonluyor, burayı açıyor ve eksik olanı açıyor.',
    none: 'Bu proje hiçbir servis beyan etmiyor ve .env dosyasından da bir şey çıkmadı.',
    declaredBy: 'stackvo.json içinde beyan edilmiş',
    suggestedBy: 'Projenin kendi .env dosyasından çıkarıldı',
    suggestedCaveat:
      'Bu bir tahmin — her birinin yanında hangi anahtardan çıktığı yazıyor. Yazmak, onu iş arkadaşlarınızın karar olarak okuyacağı bir dosyaya koyar; önce kontrol edin.',
    becauseOf: '{key} anahtarından',
    state: {
      enabled: 'Bu makinede açık',
      missing: 'Burada açık değil',
      unknown: 'Bu sürümde bu servis için şablon yok',
    },
    unknownExplained:
      'Şablonu olmayan isimler dosyadan silinmiyor, duruyor — sessizce kaybolan bir beyan kimsenin hata ayıklayamayacağı bir beyandır. Yalnızca işleme alınmıyorlar.',
    enable: '{count} servisi aç',
    enableDetail: '.env yazılır, compose dosyaları yeniden üretilir ve servisler başlatılır.',
    declare: '{count} tanesini stackvo.json’a yaz',
    written: 'Yazıldı. stackvo.json’ı commit edin; klonlayan bir sonraki kişi aynı listeyi alır.',
  },
  logs: {
    title: 'Loglar',
    live: 'canlı',
    openInEditor: 'Bu dosyayı editörde aç',
    waiting: 'Log bekleniyor…',
    liveFrom: 'buradan itibaren canlı',
    regex: 'Düzenli ifade',
    pause: 'Duraklat',
    resume: 'Devam et',
    resumeHint: 'Devam et — {n} satır bekliyor',
    clear: 'Temizle',
    clearHint: 'Görünümü temizle — diskten hiçbir şey silinmez',
    containerStream: 'Konteyner çıktısı',
    allDescription:
      'Her projeyi kapsayan canlı bir akış. Burada yalnızca bu andan sonra yazılan çıktı görünür — bir dosyanın geçmişini okumak için projesini açın.',
    allProjects: 'Bütün projeler',
    waitingAll: 'İzleniyor. Projeleriniz yazdıkça satırlar burada belirir.',
    following: '{total} dosyanın {followed} tanesi izleniyor · {projects} proje',
    files: '{n} dosya',
    group: {
      application: 'Uygulama',
      server: 'Sunucu',
    },
    search: 'Ara',
    filterLevel: 'Seviyeye göre filtrele',
    clearFilter: 'Filtreyi temizle',
    copy: 'Görünenleri kopyala',
    noMatch: 'Eşleşen yok — {n} satır gizli.',
    showing: '{total} satırın {shown} tanesi',
    level: {
      debug: 'Debug',
      info: 'Bilgi',
      notice: 'Uyarı notu',
      warning: 'Uyarı',
      error: 'Hata',
      critical: 'Kritik',
    },
  },

  hosts: {
    title: 'hosts dosyası güncellenecek',
    explain:
      'Proje alan adlarının tarayıcıdan açılabilmesi için hosts dosyasına kayıt gerekiyor. Sadece StackVo blok işaretleri arasındaki satırlar değiştirilir; dosyanın geri kalanına dokunulmaz.',
    elevation: 'Bu işlem yönetici parolası ister. Değişikliği onaylamadan hiçbir şey yazılmaz.',
    noChange: 'Değişiklik gerekmiyor — kayıtlar zaten mevcut.',
    fix: 'Kaydı ekle',
    apply: 'Uygula',
    cancel: 'Vazgeç',
  },

  terminal: {
    title: 'Terminal',
    explain:
      'Bu projenin konteyneri içinde bir kabuk, pencerenin içinde. Sistem terminali hâlâ başlıktan bir tık uzakta — bu, sayfadan çıkmadan hızlı bir bakış için.',
    needsRunning: 'Önce projeyi başlatın — kabuk konteynerin içinde çalışır.',
    start: 'Kabuk aç',
    stop: 'Kapat',
    exited: 'Kabuk sonlandı ({code}).',
  },
  repl: {
    title: 'Tezgâh',
    explain:
      'Bir parça kod yazın, bu projenin içinde uygulama açılmış hâlde çalıştırın, dönen cevabı okuyun. Tek satırlık işler için yukarıdaki terminal daha iyi — burası, üzerinde durmadan oynadığınız yirmi satır için.',
    runner: 'Şununla çalıştır',
    booted: 'uygulama açık',
    bare: 'yalnızca dil',
    snippet: 'Kod',
    placeholder: 'dump(User::count());',
    run: 'Çalıştır',
    shortcut: '⌘/Ctrl + Enter',
    needsRunning: 'Önce projeyi başlatın — kod onun konteyneri içinde çalışır.',
    printYourself:
      'Görmek istediğinizi kendiniz yazdırın — dump(), echo, print. Etkileşimli REPL’in aksine burada son ifadenin değeri kendiliğinden basılmıyor.',
    output: 'Çıktı',
    ok: 'çıkış 0',
    exit: 'çıkış {code}',
    timedOut: '30 saniyede durduruldu',
    truncated: 'çıktı kırpıldı',
    notLimited:
      'Bu imajda timeout komutu yok, bu yüzden kod konteynerin içinde sınırlanamadı. Orada hâlâ çalışıyor olabilir.',
    noOutput: 'Çalıştı ve hiçbir şey yazdırmadı.',
    history: 'Çalıştırdığınız kodlar',
    historyKeeps:
      'Kodun kendisi saklanıyor, çıktısı değil — dönen şey sizin uygulamanızın verisi. Düzenleyiciye geri koymak için birine tıklayın.',
    forget: 'Hepsini unut',
    noRunner:
      'Bu projede bir kod parçasının yükleyebileceği bir şey yok. Çalıştırıcı, ihtiyaç duyduğu dosyalar varsa sunuluyor: artisan ve laravel/tinker, wp-config.php, manage.py, bin/rails — ya da yalnızca dil için composer.json ve package.json.',
  },
  workers: {
    title: 'İşçiler',
    explain:
      'Kuyruk ve zamanlayıcı süreçleri, bu projenin kendi imajından türetilen konteynerler olarak çalışır — aynı PHP, aynı eklentiler, aynı .env. Çöken işçiyi Docker kendisi yeniden başlatır (unless-stopped), bu uygulama açık olsun ya da olmasın.',
    none: 'artisan dosyası bulunamadı — işçiler Laravel dosyalarından tespit edilir.',
    needsRunning: 'Önce projeyi başlatın — işçi, projenin derlenmiş imajını çalıştırır.',
    queue: 'Kuyruk işçisi',
    queueDesc:
      'php artisan queue:work — kuyruktaki işleri işler; bayat kodla uzun kalmamak için saatte bir yeniden başlar.',
    scheduler: 'Zamanlayıcı',
    schedulerDesc:
      'php artisan schedule:work — zamanlanmış görevleri ön planda çalıştırır; host cron kaydı gerekmez.',
    horizon: 'Horizon',
    horizonDesc:
      'php artisan horizon — Laravel Horizon süpervizörü; composer.json gerektirdiği için sunulur.',
    start: 'Başlat',
    stop: 'Durdur',
    restarts:
      'Docker bu işçiyi {count} kez yeniden başlattı — artmaya devam ederse loglarına bakın.',
  },

  tunnel: {
    scan: 'Tüneli başka bir cihazda açmak için kamerayı buna tutun. Tünel durunca bu da çalışmaz.',
    title: 'Paylaş',
    explain:
      'Bu projeye yönlenen geçici bir genel URL — .loc alan adına erişemeyen webhook gönderenler (Stripe, GitHub) için. Yığın ağında yan konteyner olarak bir Cloudflare hızlı tüneli çalıştırır; hesap gerekmez.',
    needsRunning: 'Önce projeyi başlatın — tünel projenin konteynerine yönlenir.',
    start: 'Genel URL al',
    startHint:
      'İlk başlatma cloudflared imajını indirir. URL rastgeledir, yalnızca tünel çalışırken yaşar ve her başlatmada değişir.',
    connecting: 'Bağlanıyor — Cloudflare URL atıyor…',
    stop: 'Paylaşımı durdur',
    publicWarning:
      'Bu URL genel internette canlıdır ve kimlik doğrulaması yoktur. Elinde olan herkes makinenizdeki bu projeye erişir. Test bitince paylaşımı durdurun.',
  },

  migration: {
    title: 'Servisleriniz taşınıyor',
    lead: 'Bu çalışma alanı servislerini hâlâ `.env` içinde tutuyor. Bu sürüm onları bir örnek tablosundan ve paket kataloğundan üretiyor; eski yol kaldırıldı — yani onlar taşınana kadar yığın kurulamaz.',
    reversible:
      'Önce `.env` dosyası `.env.pre-market.bak` olarak kopyalanıyor ve servis satırları yorum satırına alınıyor; bu işlem Market sayfasından geri alınabilir.',
    reading: '.env içinde ne olduğu okunuyor…',
    willKeep: 'Taşınacak olan — {count} servis, portlarını ve verilerini koruyarak:',
    blocked: 'Önce bunların çözülmesi gerekiyor',
    missing: 'Bu makinede henüz olmayan paketler',
    notInCatalogue: 'bu makinenin çektiği katalogda yok',
    nothing: '`.env` içinde açık hiçbir servis yok, yani taşınacak bir şey de yok.',
    apply: 'Taşı',
    later: 'Şimdi değil',
    laterHint:
      'Buradan çıkmak uygulamayı servissiz açar. Projeler, alan adları ve sertifikalar çalışmaya devam eder; aynı taşımayı Market sayfası da sunuyor.',
  },

  timeline: {
    title: 'İstek zaman çizelgesi',
    explain:
      'Kodun elinde ne olduğunu sandığı, veritabanına gerçekte ne sorduğu ve ne gönderdiği — tek bir eksende. Dump’lar hangi istekte olduklarını taşıyor; sorgular ve postalar taşımıyor, çünkü ne bir veritabanı günlüğü ne de bir posta yakalayıcısı kaydı hangi isteğin ürettiğini tutuyor, ve tahmin etmek iki istek ilk çakıştığında yanlış olurdu.',
    database: 'Veritabanı',
    requests: 'İstekler:',
    notRecording:
      'Sorgu günlüğü kayıtta değil, o yüzden burada yalnız dump’lar var. Üstteki panelden açın, incelediğiniz sayfayı yeniden yükleyin, sonra burayı tazeleyin.',
    empty: 'Henüz bir şey yok — incelediğiniz sayfayı yeniden yükleyin.',
  },

  queryLog: {
    title: 'Sorgu günlüğü',
    explain:
      'Veritabanına gerçekte ne soruldu, ve aynı soru satır başına bir kez nerede soruldu. Buradan açılıyor — ajan yok, yeniden derleme yok, uygulamanıza kod girmiyor.',
    database: 'Veritabanı',
    record: 'Sorguları kaydet',
    clear: 'Baştan başla',
    noTarget:
      'Bu çalışma alanında günlüğü okunabilen bir veritabanı çalışmıyor. MySQL ve MariaDB günlüğü bir tabloda tutuyor, Postgres kendi ayarlarının gösterdiği dosyaya ya da akışa yazıyor — hangisi olduğunu bu uygulama sunucuya soruyor ve biçimi sabitliyor — Mongo ise veritabanı başına bir koleksiyona profil çıkarıyor. Dördü de çalışma anında, ajansız ve yeniden derlemesiz açılıyor.',
    cost: 'Kayıt her ifadeyi örneklemeden günlüğe yazıyor ve her yazmada bir bedeli var. İşiniz bitince kapatın — bu bir ölçüm aracı değil, bir teşhis aleti. Durdurmak toplananı da siliyor, çünkü günlük ifade metnini tutuyor.',
    costPostgres:
      'Postgres’te bu ifadeler ayrıca sunucunun konteyner içindeki kendi günlük dosyasına yazılıyor. Durdurmak buradaki oturumu bitiriyor ama bu uygulama o dosyayı yeniden yazmıyor — ifade metni, sunucu dosyayı döndürene kadar orada kalıyor.',
    howTo:
      'Açın, incelediğiniz sayfayı yeniden yükleyin, sonra bakın. Tekrarlanan şekiller önce listeleniyor.',
    repeats: 'Tekrarlanan sorgular',
    noRepeats: 'Üç ya da daha fazla tekrarlanan bir şey yok.',
    nothingYet: 'Henüz kayıt yok — baktığınız sayfayı yeniden yükleyin.',
    example: 'örneğin',
    statements: 'İfadeler ({count})',
  },

  stripe: {
    title: 'Stripe webhook’ları',
    explain:
      'Canlı Stripe olaylarını bu projeye iletir. CLI dışarı doğru bağlanır; yani internetten erişilebilir olmak gerekmiyor ve imza gizi oturum boyunca değişmiyor — adresi her başlatışta değişen tünelin aksine.',
    key: 'Gizli veya kısıtlı API anahtarı',
    keyHint: 'İşletim sisteminin anahtar deposunda tutulur, çalışma alanındaki bir dosyada değil.',
    keyStored: 'Bu proje için bir anahtar saklı.',
    saveKey: 'Sakla',
    clearKey: 'Kaldır',
    path: 'İletilecek yol',
    needsRunning:
      'Önce projeyi başlatın — aksi hâlde her olay iletilemez ve Stripe bu başarısızlıkları kaydeder.',
    connecting: 'Stripe’a bağlanılıyor…',
    secretIs: 'Bu oturumun webhook imza gizi:',
    start: 'Dinle',
    stop: 'Durdur',
  },
  oauth: {
    title: 'OAuth geri dönüş adresi',
    explain:
      'Sağlayıcının konsoluna yapıştırılacak adres. Yönlendirme tarayıcıya gönderilir, sağlayıcı bu adresi kendisi çağırmaz — yani akışın kendisi için yerel adres çalışır. Değişen şey, kaydederken sağlayıcının bu metni kabul edip etmediği.',
    path: 'Geri dönüş yolu',
    local: 'Yerel adres',
    public: 'Genel adres',
    noTunnel:
      'Çalışan tünel yok, bu yüzden genel adres de yok. Sağlayıcı yereli reddederse yukarıdaki Paylaş bölümünden bir tünel başlatın.',
    takesLocal: 'Yerel yeterli',
    takesPublic: 'Genel gerekiyor',
  },
  landing: {
    title: 'Açılış sayfası',
    explain: 'Her projeyi ve servisi listeleyen tek sayfa, bu çalışma alanının kendi adresinde.',
    counts: '{projects} proje, {services} servis',
    start: 'Yayına al',
    stop: 'Durdur',
    refresh: 'Yeniden yaz',
    rendered: '{when} tarihinde yazıldı. Kendi kendini güncellemiyor.',
  },
  qr: {
    label: '{text} için QR kodu',
    tooLong: 'Bu adres bir QR koduna sığmayacak kadar uzun.',
  },
  lan: {
    scan: 'Adresi diğer cihazda açmak için kamerayı buna tutun. Aşağıdaki sertifika uyarısı orada da çıkar.',
    title: 'Bu ağda',
    explain:
      'Bu projeyi aynı ağdaki bir telefondan ya da başka bir bilgisayardan açın. Ad sslip.io üzerinden çözülüyor; adres adın kendisinden hesaplanıyor — hiçbir şey kaydedilmiyor, hiçbir şey yayınlanmıyor ve ağdan dışarı trafik çıkmıyor.',
    share: 'Diğer cihazların çözebileceği bir adla da cevap ver',
    noAddress:
      'Bu makinenin sunabileceği özel bir ağ adresi yok. Ya çevrimdışı, ya da adresi genel — ve internetteki herkesin çözebileceği bir adla yayınlanan bir geliştirme sitesi bu anahtarın işi değil.',
    certWarning:
      'Ziyaret eden tarayıcı sertifika uyarısı gösterecek. Sertifikayı bu makinenin yerel CA’sı verdi ve o cihaz onu hiç duymadı — bağlantı gerçek, ad doğru. Uyarıyı kaldırmak için CA’yı oraya kurun ya da uyarıyı geçin.',
    regenerateHint: 'Ad, bir sonraki yeniden üretimde yönlendiriciye ve sertifikaya iniyor.',
    stale:
      '{host} üretilmiş dosyalara yazılı ve bu makine artık o ağda değil. Yeniden üretin — o zamana kadar bu ad, adresi devralan makineye çözülür.',
  },

  doctor: {
    title: 'Doktor',
    sectionDesc: 'Neyin bozuk olduğu, adıyla — ve her bulgunun yanında onarımı.',
    loading: 'Yığın inceleniyor…',

    requirements: 'Başlangıç gereksinimleri',
    requirementsDesc: 'İlk ekranı tutan denetimlerin aynısı, buradan yeniden denetlenebilir.',

    coreTitle: 'Çekirdek konteynerler',
    coreDesc:
      'Her proje ve servis alan adı bunların üzerinden geçer. Bunlar çalışmıyorsa hiçbir adres cevap vermez — kurulum doğru olsa bile.',
    coreRunning: 'Çalışıyor.',
    coreStopped: 'Konteyner var ama durdurulmuş.',
    coreMissing: 'Konteyner hiç oluşturulmamış — yığın hiç başlatılmamış veya durdurulup silinmiş.',
    coreUnknown: 'Docker çalışmadığı için durumu okunamıyor.',
    coreStart: 'Çekirdek yığını başlat',

    portsTitle: 'Ana makine portları',
    portsDesc: 'Üretilen yığının talep edeceği her port ve şu anda kimde olduğu.',
    portsNone: 'Üretilen yığın hiç port yayınlamıyor — önce üreticiyi çalıştırın.',
    portFree: 'Boş.',
    portOurs: 'Yığının kendisinde ({name}).',
    portHeld: '{process} kullanıyor.',
    portHeldPid: '{process} kullanıyor (pid {pid}).',
    portHeldUnknown: 'Kullanımda, ancak süreç belirlenemedi.',
    portUnknown: 'Dinleyici tablosu okunamadı.',

    hostsTitle: 'hosts dosyası',
    hostsDesc: 'hosts kaydı olmayan bir proje alan adı, tarayıcının bulamayacağı bir sitedir.',
    hostsOk: 'Her proje alan adının kaydı var.',
    extTitle: 'PHP eklentileri',
    extDesc:
      'Üretici, kuramadığı bir eklentiyi sessizce atlar; hata daha sonra ölümcül bir “tanımsız fonksiyon” olarak ortaya çıkar.',
    extOk: 'Seçili her eklenti derlenebiliyor.',
    extDefault: '“{ext}” varsayılan seçimde ama derlenemiyor — {detail}.',
    extDefaultWhy:
      'Şimdi oluşturulacak yeni bir projede eksik olurdu. PHP {versions} sürümlerine karşı denetlendi.',
    extProject: '“{ext}” {project} projesinde derlenemiyor.',
    extOpen: 'Projeyi aç',
    extRemove: 'Kaldır',
    extRemoveHint: 'Çalışan hiçbir şey değişmez — derleme onu zaten düşürüyor.',
    hostsMissing: '{count} alan adının hosts kaydı yok.',
    hostsRepair: 'İncele ve onar',
    dnsBroken:
      'Makine {suffix} adlarını {port} portundaki yerel bir yanıtlayıcı üzerinden çözüyor ve orada cevap veren yok — o sonek altındaki her ad düşüyor.',
    dnsBrokenFix:
      'Ayarlar → Yerel DNS: yanıtlayıcıyı yeniden açın ya da makineyi ona yönlendiren anahtarı kapatın.',

    generatedTitle: 'Üretilen yapılandırma',
    generatedDesc:
      'Compose dosyaları .env ile proje manifestolarından türetilir. Bir girdiyi değiştirip yeniden üretmezseniz yığın dünün yapılandırmasıyla çalışır.',
    generatedOk: 'Girdileriyle güncel.',
    generatedStale: '{file} dosyasından eski — yığın dünün yapılandırmasıyla çalışıyor.',
    generatedMissing: 'Hiç üretilmemiş.',
    generatedUnknown: 'Çalışma alanı olmadan denetlenemez.',
    regenerate: 'Yeniden üret',

    spaceTitle: 'Disk',
    spaceDesc: 'Her yeniden derleme arkasında sahipsiz bir imaj bırakır ve bu uygulama çok derler.',
    spaceUnknown: 'Motor kapalıyken okunamaz.',
    spaceImages: '{count} kullanılmayan imaj',
    spaceVolumes: '{count} kullanılmayan birim',
    reclaim: 'Alan kazan…',
    pruneTitle: 'Disk alanı kazan',
    pruneImagesLabel: '{count} sahipsiz imajı kaldır — {size}. Tanımı gereği yeniden derlenebilir.',
    pruneVolumesLabel: '{count} kullanılmayan birimi kaldır — {size}.',
    pruneVolumesWarning:
      '“Kullanılmayan”, “şu anda bağlı değil” demektir — durdurulmuş bir projenin verisi de buna girer. Buradan kaldırılan geri gelmez; önce veritabanlarını yedekleyin.',
    pruneBuildCacheLabel: 'Build cache’in tamamını kaldır.',
    pruneBuildCacheWarning:
      'Bir projeyi silmek, o projenin imajının tuttuğu cache’i zaten geri kazanır. Geriye kalan ortak kısımdır: her proje imajı aynı PHP tabanından ve aynı eklenti kurulumlarından derlenir. Bunu kaldırmak veri kaybettirmez — her projenin bir sonraki derlemesini baştan yaptırır.',
    pruneConfirm: 'Kaldır',
    pruneResult:
      '{images} imaj, {volumes} birim ve {caches} cache kaydı kaldırıldı — {size} kazanıldı.',

    ownersTitle: 'Baytlar kimde',
    ownerCol: 'Üye',
    ownerImage: 'İmaj',
    ownerImageSize: 'İmaj boyutu',
    ownerRw: 'Yazılabilir katman',
    ownerShared: 'ortak upstream imajı',
    ownerOrphan: 'sahipsiz derleme',
  },

  newProject: {
    nameHint:
      'Küçük harf; harf veya rakamla başlar, tire, alt çizgi ve nokta kullanılabilir (ör. api.myapp).',
    domainHint: 'Boş bırakılırsa proje adından üretilir.',
    domain_https:
      'Bu uzantı tarayıcıların HSTS ön yükleme listesinde: yalnızca HTTPS ile açılır ve uyarıyı geçme imkânı yoktur. Önce Ayarlar’dan HTTPS’i aç.',
    domain_certificate:
      'Yapılandırılmış soneğin dışında, joker sertifika bunu kapsamıyor — projeyi oluşturduktan sonra sertifikaları yeniden üret.',
    documentRootHint: 'Proje köküne göre yol.',
    portHint: 'Uygulamanın konteyner içinde dinlediği port.',
    sectionProject: 'Proje',
    sectionPhp: 'PHP yapılandırması',
    sectionNode: 'Node yapılandırması',
    sectionLang: '{runtime} yapılandırması',
    langVersion: 'Sürüm',
    optionalStep: 'İsteğe bağlı — bu adımı atlamak için boş bırakın.',
    langBindHint: '0.0.0.0 ve yukarıdaki portu dinlemeli; Traefik ona yönlendirir.',
    title: 'Yeni proje',
    name: 'Proje adı',
    template: 'Başlangıç',
    templates: {
      empty: 'Boş proje',
      git: 'Git deposundan çek',
      laravel: 'Laravel',
      wordpress: 'WordPress',
      symfony: 'Symfony',
      nextjs: 'Next.js',
      nuxt: 'Nuxt',
      vue: 'Vue (Vite)',
      react: 'React (Vite)',
      svelte: 'SvelteKit',
      astro: 'Astro',
      cakephp: 'CakePHP',
      yii: 'Yii 2',
      codeigniter: 'CodeIgniter 4',
      laminas: 'Laminas (Zend)',
      drupal: 'Drupal',
      prestashop: 'PrestaShop',
      django: 'Django',
      rails: 'Ruby on Rails',
      slim: 'Slim',
      nest: 'NestJS',
      tina: 'TinaCMS',
      angular: 'Angular',
      typo3: 'TYPO3',
      gin: 'Gin',
      echo: 'Echo',
      flask: 'Flask',
      fastapi: 'FastAPI',
      sinatra: 'Sinatra',
      rocket: 'Rocket',
    },
    templateGroups: {
      php: 'PHP',
      node: 'JavaScript',
      cms: 'CMS & e-ticaret',
      python: 'Python',
      go: 'Go',
      other: 'Ruby & Rust',
    },
    detectedHint:
      'Çalışma ortamı, web sunucusu ve doküman kökü kurucunun yazdığı dosyalardan belirlenir — Laravel public/ üzerinden, WordPress proje kökünden servis eder. Sonrasında proje ayarlarından değiştirilebilir.',
    templateHint:
      'Çerçevenin kendi kurucusu geçici bir konteynerde çalışır; sonra tespit, yazdıklarından projeyi yapılandırır. İlk çalıştırma kurucu imajını indirir — birkaç dakika verin.',
    gitUrl: 'Depo adresi',
    gitUrlPlaceholder: "git{'@'}sunucu.example.com:grup/alt-grup/depo.git",
    gitUrlHint:
      'SSH veya HTTPS klon adresi. Herhangi bir sunucu olabilir — kendi GitLab’ınız da dâhil.',
    gitAuthHint:
      'Klonlama bilgisayarınızdaki git ile yapılır. Anahtar, ssh yapılandırması ve sunucu izinleri sizin kurulumunuzdan okunur — StackVo bunların hiçbirini yönetmez. Terminalde çalışan bir adres burada da çalışır.',
    gitManifestHint:
      'Depoda stackvo.json varsa ayarları olduğu gibi kullanılır — takımın cevabı sizindir, yukarıdaki alanlar yok sayılır. Yoksa proje, gelen dosyalardan tespit edilerek yapılandırılır.',
    aliases: 'Ek alan adları',
    aliasesHint:
      "Bu projenin cevap verdiği diğer adlar. stackvo.json'a yazılır, yani klonlayan bir iş arkadaşı da alır.",
    aliasesWildcard:
      'Joker, sertifikaya ve yönlendiriciye ulaşır ama hiçbir hosts dosyası joker ifade edemez — o adlar siz elle eklemedikçe çözülmez.',
    domain: 'Alan adı',
    runtime: 'Çalışma ortamı',
    phpVersion: 'PHP sürümü',
    nodeVersion: 'Node sürümü',
    packageManager: 'Paket yöneticisi',
    packageManagerNone: 'Sabitlenmemiş (imajın getirdiği npm)',
    packageManagerHint:
      'İmajda Corepack’i etkinleştirir; package.json’daki `packageManager` alanının bir sürümü sabitlemesini sağlayan şey budur. Sabitlemeden bırakmak imajı eskisiyle birebir aynı kurar.',
    server: 'Web sunucusu',
    documentRoot: 'Doküman kökü',
    extensions: 'PHP eklentileri',
    incompatible: 'Bu PHP sürümüyle kurulamaz',
    tooManyExtensions: 'katalogda olandan fazla eklenti',
    install: 'Kurulum komutu',
    build: 'Derleme komutu (opsiyonel)',
    start: 'Başlatma komutu',
    port: 'Port',
    bindHint: '0.0.0.0 adresine bağlanmalı, yoksa Traefik erişemez.',
    create: 'Oluştur',
    unavailableRuntimes: 'Generator olmadığı için gizlendi: {list}',
    deleteTitle: '{name} silinsin mi?',
    deleteBody: 'Proje StackVo listesinden çıkar. Kaynak dosyalar diskte kalır.',
    deleteAlso:
      'Konteyneri, imajı, üretilen Dockerfile’ı, logları, hosts kaydı ve sertifikadaki adı da kaldırılır.',
    deleteFiles: 'Proje klasörünü de sil (geri alınamaz)',
    delete: 'Sil',
  },

  projectSettings: {
    title: '{name} yapılandırması',
    open: 'Yapılandır',
    nameLocked: 'Klasör adı projenin kimliğidir; yeniden adlandırmak klasörü taşımak demektir.',
    extensionUnknown: 'Bu projenin istediği bir eklenti, katalogda yok',
    domainChanged:
      'Hosts kaydı ve sertifika hâlâ eski alan adını taşıyor. Değişiklik uygulandıktan sonra ikisi de önerilir.',
    applyPending:
      'Kaydedildi. Dosyalar yeniden üretilip imaj derlenene kadar konteyner önceki yapılandırmayla çalışmaya devam eder.',
    applyNow: 'Şimdi uygula',
    saveAndApply: 'Kaydet ve uygula',
    engineDown:
      'Docker çalışmıyor, bu yüzden hiçbir şey yeniden derlenemez. Kaydet, değişikliği diskte tutar.',
  },

  detail: {
    openFolder: 'Klasörü aç',
    dockerfileDesc: 'Rust üreteci bu projeyi nasıl render ediyor — dosyaya yazmadan.',
    compatHint: 'Üreticinin gerçekte yazdığı hâli; kurulamayan eklentiler sessizce atlanır.',
    strictHint: 'Kurulamayan bir eklenti varsa üretmeyi reddeder ve hangisi olduğunu söyler.',
    notBuilt: 'Konteyner henüz derlenmedi; log akışı için önce derleyin.',
    openInEditor: 'Editörde aç',
    externalTerminal: 'Harici terminalde aç',
    rebuildHint:
      'Yeniden derle: Dockerfile stackvo.json’dan yeniden üretilir, imaj derlenir ve konteyner yeniden yaratılır. Yeniden başlatmak bunların hiçbirini yapmaz — aynı imajdan aynı konteyneri verir.',
    manifest: 'Manifest',
    manifestHint: 'stackvo.json — kaydedince anahtar sırası sözleşmeye göre düzeltilir.',
    save: 'Kaydet',
    bringUp: 'Compose ile ayağa kaldır',
    dockerfile: 'Dockerfile',
    image: 'İmaj',
    state: 'Durum',
    matchesGenerated: 'Üretilmiş dosya güncel',
    generatedStale: 'Üretilmiş dosya bayat — yeniden üretin',
    strict: 'Katı',
    compat: 'Üretilen',
    silentlySkipped: 'Normal render bunları sessizce atlıyor',
  },

  // Suggestions, keyed by `hintKey` on the error the Rust side raised.
  //
  // The catalogue is `src-tauri/src/hints.rs`; these are its translations, and
  // `src-tauri/tests/hint_translations.rs` fails the build if the two sets ever
  // differ in either direction — a hint with no translation, or a translation
  // for a hint nothing raises any more.
  //
  // The English in en.js is a copy of what the Rust carries as its fallback,
  // and the same test pins the two equal. Deliberate: it turns an edit to the
  // English into a change that has to pass through the translations, instead of
  // one that silently leaves Turkish describing the old behaviour.
  errorHints: {
    startDocker: "Docker Desktop'ı başlatıp tekrar deneyin.",
    startDockerOrSetHost:
      "Docker Desktop'ı başlatın; motor başka bir yerdeyse DOCKER_HOST değişkenini ayarlayın.",
    startDockerManually: "Docker'ı elle başlatıp tekrar deneyin.",
    projectMayNotBeBuilt: 'Proje henüz derlenmemiş olabilir.',
    chooseWorkspace:
      "StackVo'nun kuracağı boş bir klasör seçin ya da hâlihazırda yönettiği bir klasörü gösterin.",
    projectNameCharset:
      'Adlar harf, rakam, nokta, alt çizgi ve tire içerebilir; harf veya rakamla başlamalıdır.',
    pathLeavesProjects: 'Proje klasörünün dışına çıkan bir yol üzerinde işlem yapılmıyor.',
    onlyProjectFolders: 'Yalnızca seçili çalışma alanı içindeki proje klasörleri açılabilir.',
    adoptInstead: 'Bunun yerine projeyi devralın — manifesti yazan yol odur.',
    fixOrAdopt: 'Dosyayı düzeltin ya da silip klasörü devralın.',
    runDoctorThenRetry:
      'Ayarlar → Doktor neyin bozuk olduğunu listeler ve onarabilir; sonra tekrar klonlayın veya kaydedin.',
    adoptExistingCode:
      'Mevcut kod için devralmayı kullanın — iskelet kurma sıfırdan bir proje içindir.',
    chooseAnotherName: 'Başka bir ad seçin ya da orada duran klasörü devralın.',
    installGitOrAdopt: 'git kurun ya da depoyu kendiniz klonlayıp klasörü devralın.',
    editFromManifestTab: 'Bunun yerine projenin Manifest sekmesinden düzenleyin.',
    startProjectForCommands: "Önce projeyi başlatın — bu komutlar onun container'ı içinde çalışır.",
    replRunnerNeedsFiles: 'Bir çalıştırıcı, yalnızca projede yüklediği dosyalar varsa sunulur.',
    buildAndStartForWorker: 'Önce projeyi derleyip başlatın — worker onun imajıyla çalışır.',
    workersAreDetected: "Worker'lar artisan ve composer.json üzerinden tespit edilir.",
    startProjectForTunnel: "Önce projeyi başlatın — tünel onun container'ına yönlendirir.",
    worktreeIsDirty:
      'Worktree’de commit edilmemiş değişiklikler var. Bunları commit ya da stash edin; ya da onları atan Zorla seçeneğiyle kaldırın.',
    databaseNameCharset:
      'Veritabanı adları küçük harf, rakam ve alt çizgi içerebilir ve bir harfle başlamalıdır.',
    mongoHasNoSourceDatabase:
      'Worktree’yi boş bir veritabanıyla oluşturun — MongoDB ilk yazmada kendisi oluşturur.',
    installMkcert:
      "mkcert'i kurun: macOS'ta `brew install mkcert`, Linux'ta paket yöneticinizle, Windows'ta `choco install mkcert`. Sonra tekrar deneyin.",
    checkTldAndDomains:
      '.env içindeki DEFAULT_TLD_SUFFIX değerini ve her stackvo.json dosyasındaki `domain` alanını kontrol edin.',
    certificateIssuedButUntrusted:
      'Sertifika her hâlükârda üretildi ve stack hizmet veriyor — otorite güvenilir sayılana kadar tarayıcı yayıncı hakkında uyarır. Ayarlar → Sertifikalar altında bunu sizin terminalinizde yapan bir düğme var; parola sorusu orada yanıtlanabilir.',
    runMkcertInstall:
      'Bir terminalde bir kez `mkcert -install` çalıştırın — sistem güven deposu için parola ister ve pencereli bir uygulamanın soracağı bir terminali yoktur.',
    hostnameCharset: 'Alan adları harf, rakam, nokta ve tire içerebilir.',
    hostsNeedsAdmin: 'hosts dosyasını düzenlemek için yönetici yetkisi gerekiyor.',
    hostsNotReplaced: 'hosts dosyası değiştirilemedi.',
    installPolkit: 'polkit kurun ya da /etc/hosts dosyasını elle düzenleyin.',
    perfPathIsRelative: 'Proje içinden bir dizin adı verin: vendor, storage/framework gibi.',
    perfNothingToSeed:
      'O dizin projede henüz yok. Önce bağımlılıkları kurun ya da açıp konteyner içindeki araçların oluşturmasına izin verin.',
    perfSeedFailed: 'Dizin birime kopyalanamadı, bu yüzden hiçbir şey değiştirilmedi.',
    tldIsOneLabel: 'Sonek tek bir etiketle biter: harf, rakam ve tire — stackvo.loc gibi.',
    dnsPlaceTheLineYourself:
      'Gösterilen satırı bu makinede adları çözen şeye ekleyin ve onu yeniden yükleyin.',
    dnsStartTheResponderFirst:
      'Önce yanıtlayıcıyı başlatın — aksi hâlde makine kapalı bir porta yönlendirilirdi.',
    dnsMachineIsNotAskingUs:
      'Yanıtlayıcı cevap veriyor ama makine ona sormuyor. Çözümleyicinin önünde başka bir şey olabilir.',
    dnsPublicNamesStopped:
      'Değişiklik genel adları da götürdü ve geri alındı. Geride hiçbir şey bırakılmadı.',
    dnsPortAlreadyAnswering: 'Bu makinede o portta zaten başka bir şey cevap veriyor.',
    serviceMustBeInCatalog:
      'Yalnızca contracts/env.schema.json içinde listelenen servisler yönetilebilir.',
    snapshotNameCharset:
      'Harf, rakam, nokta, tire ve alt çizgi kullanın — ad bir dosya adına dönüşüyor. `auto-` zamanlanmış snapshot’lara ayrılmıştır.',
    snapshotNameInUse:
      'Başka bir ad seçin ya da önce mevcut snapshot’ı silin — bir snapshot asla yerinde ezilmez.',
    supportedDatabases: 'Desteklenenler: mysql, mariadb, postgres, mongo.',
    enableAMailCatcher:
      '.env içinde mailhog (ya da mailpit) servisini etkinleştirip yeniden üretin.',
    mailUiMayBeStarting:
      'Container hâlâ başlıyor olabilir ya da arayüz portu başkası tarafından tutuluyor olabilir.',
    envKeyCharset:
      'Anahtarlar ^[A-Z_][A-Z0-9_]*$ kalıbına uymalı ki Compose onları yerine koyabilsin.',
    envIsOneKeyPerLine:
      '.env biçimi satır başına bir anahtardır; çok satırlı değerler geri okunamaz.',
    revealValueFirst: 'Önce değeri görünür yapın ya da alana dokunmayın.',
    settingIsRequired: 'Paket bu ayarı zorunlu işaretliyor — servis o olmadan başlamaz.',
    portHeldByInstance:
      'Bu portu başka bir instance yayınlıyor. Önce onu değiştirin ya da başka bir numara seçin.',
    portInUse: 'Bu makinede zaten bir şey orayı dinliyor. Başka bir numara seçin.',
    phpIniDirectiveCharset: 'Direktif adları harf, rakam, alt çizgi ve noktadan oluşur.',
    phpIniIsOnePerLine: 'php.ini satır başına bir direktiftir.',
    phpIniSizeFormat:
      'Boyutlar, isteğe bağlı K, M veya G ekiyle bir sayıdır — 256M, 1G, 512. Süreler tam saniyedir. -1 sınırsız demektir.',
    serverDirectivesUnsupported:
      'Yalnızca nginx, caddy ve frankenphp için direktif eklenebilecek üretilmiş bir yapılandırma var.',
    unlockTheKeystore:
      'Anahtar zincirinizi açıp yeniden deneyin — bu ayarın şifresi orada saklanıyor.',
    onlyCredentialsMove:
      'Anahtar deposunda yalnızca şifreler, token’lar ve sunucu kimlikleri tutulabilir.',
    agentConfigUnparseable:
      'Bu dosya düz JSON değil — birkaç editör içinde yorum satırına izin veriyor ve bunlar silinmeden dosya güvenle düzenlenemez. Dosyayı açıp burada gösterilen bloğu yapıştırın.',
    buildTheMcpServer:
      'Önce derleyin: StackVo checkout’unda `cargo build --release --bin stackvo-mcp`.',
    keystoreEntryIsGone:
      'Giriş anahtar deposundan silinmiş. Servisi geri getirmek için değeri yeniden girin.',
    settingIsManaged:
      'Bu değer, bu makinedeki bir politika dosyasından geliyor. Makineyi yöneten kişiye danışın.',
    presetIsExportedJson:
      'Bir hazır ayar, Ayarlar → Hazır ayarlar bölümünün dışa aktardığı JSON dosyasıdır.',
    presetWrongFile:
      'İçe aktarıcıya başka bir JSON dosyası gösterilmiş olması en sık görülen sebeptir.',
    presetTooNew:
      "StackVo Desktop'ı güncelleyin ya da daha eski bir sürümle dışa aktarılmış bir hazır ayar isteyin.",
    onlyShippedTemplates: 'Yalnızca uygulamanın birlikte geldiği şablonlar geçersiz kılınabilir.',
    revertTemplateFirst: 'Uygulamayla gelen sürümü geri istiyorsanız önce değişikliği geri alın.',
    profileIdsFromList: 'Profil kimlikleri, profile_list çıktısındaki cachegrind.out.* adlarıdır.',
    profileIsCompressed:
      'Xdebug varsayılan olarak sıkıştırır; StackVo profillemeyi açarken bunu kapatır. Bu profili yeniden kaydedin ya da dosyayı kendiniz gunzip ile açın.',
    logIdsAreRelative: 'Günlük kimlikleri görecelidir; üst dizin ya da kök parçası içeremez.',
    installATerminal: 'Birini kurun ya da yerleşik terminali kullanın.',
    chooseABrowser: 'Ayarlar → Dış uygulamalar bölümünden bir tarayıcı seçin.',
    chooseAnEditor: "Ayarlar'dan bir düzenleyici seçin ya da klasörü elle açın.",
    migrateTheWorkspace:
      'Bu çalışma alanının servislerini .env dışına taşıyın — uygulama bir sonraki açılışta bunu öneriyor, aynı taşımayı Market sayfası da sunuyor. Geri alınabilir.',
    servicePublishesNothing:
      'Servisi başlatın ya da bir port yayınladığını doğrulayın — yalnızca Docker ağından erişilebilen bir konteynerin, bu makinedeki bir istemcinin kullanabileceği adresi yoktur.',
    chooseADbClient:
      'Bu tür adresleri açan bir istemci kurun ya da bağlantı dizesini kopyalayıp kendiniz yapıştırın.',
    waitForOperation: 'Bitmesini bekleyin ya da ilerlemeyi işlem konsolundan izleyin.',
    noRegistryKey:
      'Bu derleme hiçbir registry anahtarı pinlemiyor. Kendi aynasını çalıştıran bir kurum market.registryKey politikasıyla bir tane pinleyebilir.',
    signedByUnknownKey:
      'Dizin başka bir yerden geliyor olabilir ya da yayıncı, bu makine yenisini öğrenmeden anahtar değiştirmiş olabilir.',
    packageVersionRevoked:
      'Yayıncı bu sürümü geri çekti. Başka bir sürüm seçin ya da gerekçesini registry kaydında okuyun.',
    quickCommandsAreFixed:
      'Kimlikler ya gömülü katalogdan ya da bu projenin kendi stackvo.json dosyasından gelir; serbest değildir.',
    imageReferenceCharset: 'Yalnızca küçük harf, rakam ve . _ - / : karakterleri.',
    composeFileNotFound:
      'compose.yaml, compose.yml, docker-compose.yaml ve docker-compose.yml dosyalarına bakıldı.',
    composeFileMustBeValid:
      'Dosya `docker compose config` ile çözümleniyor, dolayısıyla geçerli bir Compose dosyası olmalı — içinde yerine koyduğu değişkenler dahil.',
    useGenerateRun:
      'generate_run kullanın; `verify` modu diskteki duruma göre sapmayı yine de raporlar.',
    mcpNeedsAllowWrites:
      'Yazma araçlarını etkinleştirmek için --allow-writes ile yeniden başlatın.',
    portRangeExhausted:
      'Bu servisin istediği portun yakınında bir port boşaltın, ya da örneğe ayarlarından açıkça bir port verin.',
    packagePathsStayInside: 'Bir paket yalnızca kendi dizinindeki dosyaları adlandırabilir.',
    packageContentChanged: 'Paketi yeniden kurun; dosyaları, manifestin yazıldığı dosyalar değil.',
    packageNotInstalled: 'Bu sürümün paketini kurun, ya da ona ihtiyaç duyan örneği kaldırın.',
    packageRefusedByPolicy:
      "Bu paket, StackVo'nun bir pakete vermediği bir şey istiyor. Yayınlayan kişiye bildirin.",
    packageNotInRegistry: 'Katalogu yenileyin, ya da listelediği bir sürüm seçin.',
    bundleNeedsAnEmptyDirectory:
      'Henüz var olmayan bir dizin seçin, ya da boş bir tane — başkasının dosyalarının üstüne yazılmış bir paket, kimsenin içeriğinden emin olamayacağı bir pakettir.',
    registryWentBackwards:
      'Bu kaynağın sunduğu katalog, burada olandan daha eski. Kullanmadan önce kaynağı kontrol edin.',
    registryUnreachable:
      'Katalog çekilemedi. Adresi ve bu makinenin oraya erişip erişmediğini kontrol edin — sistem ayarlarındaki proxy kullanılıyor.',
    registryAddressIsADirectory:
      'Adres, registry.json’ı barındıran dizin olmalı — onun üstündeki sayfa değil. GitHub depo adresi otomatik çevrilir; diğer her adres verildiği gibi kullanılır.',
    registryMustBeHttps:
      'Katalog adresi https:// ile başlamak zorunda. Henüz hiçbir şey imza doğrulamıyor, yani korumanın tamamı taşıma katmanı.',
    removeTheInstanceFirst: 'Bu paketi hâlâ bir örnek kullanıyor. Önce onu kaldırın, sonra paketi.',
    serviceIsSingleInstance:
      'Bu servis aynı anda tek sürüm çalıştırır. Önce elinizdeki örneği kaldırın.',
  },

  errors: {
    NETWORK_ERROR: 'Ulaşılması gereken bir sunucu cevap vermedi.',
    ENGINE_UNREACHABLE: 'Docker motoruna ulaşılamıyor.',
    NO_WORKSPACE: 'StackVo dizini seçilmedi.',
    IO_ERROR: 'Dosya işlemi başarısız oldu.',
    NOT_FOUND: 'Bulunamadı.',
    ALREADY_EXISTS: 'Bu isimde bir proje zaten var.',
    INVALID_INPUT: 'Girdi geçersiz.',
    INVALID_MANIFEST: 'stackvo.json sözleşmeye uymuyor.',
    UNSUPPORTED: 'Bu özellik v1 sürümünde desteklenmiyor.',
    GENERATE_FAILED: 'Üretim başarısız oldu.',
    BUILD_FAILED: 'Derleme başarısız oldu.',
    PERMISSION_DENIED: 'Yetki verilmedi.',
    FORBIDDEN: 'Bu makinedeki bir politika buna izin vermiyor.',
    CONFLICT: 'Bu işlem zaten çalışıyor.',
    UNKNOWN: 'Beklenmeyen bir hata oluştu.',
  },
};
