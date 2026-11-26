# Discord Quest Helper — Türkçe

Discord'un oyun oynama süresi görevleri için küçük Windows exe dosyaları oluşturur. Yardımcı dosyaları uygulama içinden başlatabilir ve durdurabilirsiniz.

## Kullanım

1. [Releases bölümünden](https://github.com/Efeumut0/discord-quest-helper/releases) Windows x64 portable paketini indirin.
2. Arşivi yazılabilir bir klasöre çıkarıp `DiscordQuestHelper.exe` dosyasını açın.
3. Discord masaüstü uygulamasında hesabınızı açın ve ilgili görevi kabul edin.
4. Oyunu arayın; varsa Discord'un Windows dosya adlarını kullanan **1. kaynağı** tercih edin.
5. Oluşturup başlatın ve ilerlemeyi Discord'un görev sayfasından kontrol edin.
6. İşiniz bitince yardımcı süreci durdurun.

1. kaynak, Discord kataloğundaki dosya adlarını kullanır. 2. kaynak ise tahmini adlarla çalışır ve RPC üzerinden profil görünümü gönderir. Profilde oyunun görünmesi görevin ilerlediği anlamına gelmez. Algılama ve görev tamamlama garanti edilmez. Uygulama Discord tokenınızı istemez.

Oyun yardımcıları exe'nin yanındaki `games/<uygulama-id>/` klasörüne yazılır. `metadata/`, `data/` ve `resources/` gerektiğinde oluşturulur. WebView2 Runtime gerekir. Otomatik güncelleyici bulunmaz.

## Derleme

Windows x64, PowerShell 7, Node.js 22, Corepack, Rust stable, Visual Studio C++ Build Tools ve Windows SDK gerekir.

```powershell
corepack enable
pwsh -File scripts/build.ps1
```

Script testleri ve Clippy'yi çalıştırır; runner'ı gömerek gerçek Tauri portable build üretir. Varsayılan çıktı kaynak klasörünün dışındaki `../discord-quest-helper-win32-x64/DiscordQuestHelper.exe` dosyasıdır. Tarihli kaynak klasörlerinde tarih eki korunur. Installer oluşturulmaz.

Geliştirici: [efeumut](https://github.com/Efeumut0). MIT lisansı ve özgün telif bildirimleri korunur. Discord ile resmî bağlantısı veya Discord'un onayı yoktur.
