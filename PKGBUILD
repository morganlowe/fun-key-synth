pkgname=fun-key-synth
pkgver=1.4.1
pkgrel=1
pkgdesc="Groovebox and Party DJ Synthesizer"
arch=("x86_64")
url="https://github.com/morganlowe/fun-key-synth"
license=("MIT")
depends=("alsa-lib" "gcc-libs")

package() {
  cd "$startdir"
  install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"
  install -Dm644 "packaging/fun-key-synth.png" "$pkgdir/usr/share/icons/hicolor/256x256/apps/$pkgname.png"
  printf "%s\n" "[Desktop Entry]" "Type=Application" "Name=Fun Key Synth" "Comment=Groovebox and Party DJ Synthesizer" "Exec=fun-key-synth" "Icon=fun-key-synth" "Terminal=false" "Categories=AudioVideo;Audio;Music;Synthesizer;" | install -Dm644 /dev/stdin "$pkgdir/usr/share/applications/$pkgname.desktop"
}
