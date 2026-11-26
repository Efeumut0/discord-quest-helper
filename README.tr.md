# Discord Quest Helper — Türkçe

Discord'un oyun oynama süresi görevleri için küçük Windows exe dosyaları oluşturur. Yardımcı dosyaları uygulama içinden başlatabilir ve durdurabilirsiniz.

## Kullanım

1. [Releases bölümünden](https://github.com/Efeumut0/discord-quest-helper/releases) Windows x64 `setup.exe` dosyasını indirin.
2. Kurulumu tamamlayıp Başlat menüsünden Discord Quest Helper'ı açın.
3. Discord masaüstü uygulamasında hesabınızı açın ve ilgili görevi kabul edin.
4. Oyunu arayın; varsa Discord'un Windows dosya adlarını kullanan **1. kaynağı** tercih edin.
5. Oluşturup başlatın ve ilerlemeyi Discord'un görev sayfasından kontrol edin.
6. İşiniz bitince yardımcı süreci durdurun.

1. kaynak, Discord kataloğundaki dosya adlarını kullanır. 2. kaynak ise tahmini adlarla çalışır ve RPC üzerinden profil görünümü gönderir. Profilde oyunun görünmesi görevin ilerlediği anlamına gelmez. Algılama ve görev tamamlama garanti edilmez. Uygulama Discord tokenınızı istemez.

Oyun yardımcıları `%LOCALAPPDATA%/DiscordQuestHelper/games/<uygulama-id>/` klasörüne yazılır. Metadata ve önbellek aynı veri klasöründe, kurulum dosyalarından ayrı tutulur. Eksikse WebView2 Runtime kurulum sırasında yüklenir.

Uygulama açılışta GitHub'daki yeni sürümü kontrol eder. Güncelleme varsa indirir, imzasını doğrular ve arka planda yükleyip yeniden açılır. Bağlantı hatası uygulamayı kullanmanızı engellemez.

## Derleme

Windows x64, PowerShell 7, Node.js 22, Corepack, Rust stable, Visual Studio C++ Build Tools ve Windows SDK gerekir.

```powershell
corepack enable
pwsh -File scripts/build.ps1 -Unsigned
```

Script testleri ve Clippy'yi çalıştırır, runner'ı gömer ve NSIS setup üretir. Çıktı `../Discord Quest Helper Releases/<sürüm>/` klasöründedir. `-Unsigned` yerel deneme içindir; resmî yayınlar güncelleme anahtarıyla imzalanır.

## Sürüm ve yayın

`2026.11.2601`, 26 Kasım 2026 gününün ilk yayınıdır. Aynı gün ikinci yayın `2026.11.2602` olur. Tam tarih uygulamada ve yayın başlığında ayrıca gösterilir.

Actions içindeki **Publish Windows release** işlemi sürümü belirler, kontrolleri çalıştırır, setup ve imzasını üretir. Tüm dosyalar hazır olduğunda kullanım talimatlarıyla yayınlar. Başarısız build güncelleme olarak dağıtılmaz.

Geliştirici: [efeumut](https://github.com/Efeumut0). MIT lisansı ve özgün telif bildirimleri korunur. Discord ile resmî bağlantısı veya Discord'un onayı yoktur.
