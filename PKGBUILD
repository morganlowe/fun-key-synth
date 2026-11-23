pkgname=fun-key-synth
pkgver=1.4.2
pkgrel=1
pkgdesc="Fun Key Synth — Vibe Studio, Auto-Chord/Beat Groovebox & 8-Part Performance Sampler"
arch=('x86_64')
url="https://github.com/morganlowe/fun-key-synth"
license=('GPL-3.0-or-later')
depends=('alsa-lib' 'libxkbcommon' 'wayland' 'libglvnd')
options=('!debug' '!lto')

package() {
  cd "$startdir"
  install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"
  install -Dm644 "packaging/fun-key-synth.desktop" "$pkgdir/usr/share/applications/$pkgname.desktop"
  install -Dm644 "packaging/fun-key-synth.png" "$pkgdir/usr/share/icons/hicolor/256x256/apps/$pkgname.png"
  install -Dm644 "README.md" "$pkgdir/usr/share/doc/$pkgname/README.md"
  if [ -f "LICENSE" ]; then
    install -Dm644 "LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
  fi
}
