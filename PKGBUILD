pkgname=fun-key-synth
pkgver=1.3.2
pkgrel=1
pkgdesc="Groovebox and Party DJ Synthesizer"
arch=("x86_64")
url="https://github.com/morganlowe/fun-key-synth"
license=("MIT")
depends=("alsa-lib" "gcc-libs")

package() {
  cd "$startdir"
  install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"
  printf "%s\n" "[Desktop Entry]" "Type=Application" "Name=Fun Key Synth" "Comment=Groovebox and Party DJ Synthesizer" "Exec=fun-key-synth" "Icon=audio-x-generic" "Terminal=false" "Categories=AudioVideo;Audio;Music;Synthesizer;" | install -Dm644 /dev/stdin "$pkgdir/usr/share/applications/$pkgname.desktop"
}
