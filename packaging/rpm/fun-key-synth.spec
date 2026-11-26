%global debug_package %{nil}
%global _build_id_links none
%{!?_version:%define _version 1.5.0}
%{!?_source_dir:%define _source_dir %{_builddir}}
%{!?_binary_path:%define _binary_path %{_source_dir}/target/release/fun-key-synth}

Name:           fun-key-synth
Version:        %{_version}
Release:        1
Summary:        Beginner-friendly analog vibe studio & 8-part performance sampler
License:        GPL-3.0-or-later
URL:            https://github.com/morganlowe/fun-key-synth
ExclusiveArch:  x86_64

Requires:       libxkbcommon.so.0()(64bit)
Requires:       libEGL.so.1()(64bit)
Requires:       libGL.so.1()(64bit)

%description
Fun Key Synth is a standalone desktop music studio built in Rust featuring
a Vibe Studio, Auto-Chord / Auto-Beat groovebox, local Party Mode song
generator, Sound Lab hardware synth rack, and Volca-style 8-part performance
sampler.

%install
rm -rf %{buildroot}
install -Dm755 "%{_binary_path}" "%{buildroot}%{_bindir}/fun-key-synth"
strip --strip-unneeded "%{buildroot}%{_bindir}/fun-key-synth"
install -Dm644 "%{_source_dir}/packaging/fun-key-synth.desktop" "%{buildroot}%{_datadir}/applications/fun-key-synth.desktop"
install -Dm644 "%{_source_dir}/packaging/fun-key-synth.png" "%{buildroot}%{_datadir}/icons/hicolor/256x256/apps/fun-key-synth.png"
install -Dm644 "%{_source_dir}/README.md" "%{buildroot}%{_docdir}/%{name}/README.md"
install -Dm644 "%{_source_dir}/LICENSE" "%{buildroot}%{_licensedir}/%{name}/LICENSE"

%files
%{_bindir}/fun-key-synth
%{_datadir}/applications/fun-key-synth.desktop
%{_datadir}/icons/hicolor/256x256/apps/fun-key-synth.png
%dir %{_docdir}/%{name}
%{_docdir}/%{name}/README.md
%dir %{_licensedir}/%{name}
%{_licensedir}/%{name}/LICENSE
