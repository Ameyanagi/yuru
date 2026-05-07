# Yuru

Yuru는 일본어 읽기 검색, 한국어 한글 검색, 중국어 병음 검색을 지원하는 빠른 명령줄 fuzzy finder입니다.
fzf와 비슷한 사용감을 유지하면서 CJK 텍스트의 phonetic match와 정확한 하이라이트를 목표로 합니다.

## Demo Video

[Yuru command demo 보기](../demo.mp4)

<video src="../demo.mp4" controls muted playsinline width="100%"></video>

## 설치

Yuru는 기본적으로 사용자 영역에 설치됩니다. `sudo`가 필요하지 않습니다.

macOS / Linux 대화형 설치:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.8/install | sh -s -- --all --version v0.1.8
```

기본 설치 위치는 `~/.local/bin`입니다. `XDG_BIN_HOME` 또는 `YURU_INSTALL_BIN_DIR`로 변경할 수 있습니다.
이 명령은 대화형 터미널에서 기본 언어, preview command, preview text extensions,
이미지 preview protocol, shell binding, shell path backend를 물어보고
`~/.config/yuru/config.toml`에 저장합니다.
Enter를 누르면 각 항목의 기본값을 사용합니다. preview command 기본값 `auto`는 text preview에
`bat`이 있으면 쓰고, 이미지는 내부 preview를 사용합니다. 이미지 preview protocol 기본값은
`none`입니다. shell path backend 기본값 `auto`는 `fd`, `fdfind`, fallback 순서로 사용합니다.

대화형 설치의 기본값을 명시하려면:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.8/install | sh -s -- --all --version v0.1.8 --default-lang none --preview-command auto --preview-image-protocol none --path-backend auto --bindings all
```

나중에 바꾸려면 `yuru configure`를 실행합니다.

Windows PowerShell:

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.8/install.ps1
Invoke-Expression "& { $script } -All -Version v0.1.8"
```

`%LOCALAPPDATA%\Yuru\bin`에 `yuru.exe`를 설치하고, 사용자 PATH와 PowerShell profile을 업데이트합니다.
대화형 환경에서는 기본 언어, preview command, preview text extensions, 이미지 preview protocol, shell binding, shell path backend를 물어봅니다. `-DefaultLang none`, `-PreviewCommand auto`, `-PreviewImageProtocol none`, `-PathBackend auto`, `-Bindings all`처럼 지정하면 대화형 설치의 기본값을 명시할 수 있습니다.

바이너리만 설치:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.8/install | sh -s -- --version v0.1.8
```

crates.io에서 설치:

```sh
cargo install yuru
```

crates.io package 이름과 설치되는 명령은 모두 `yuru`입니다.
소스 빌드는 일본어 읽기를 위해 Lindera embedded IPADIC을 사용하므로 C compiler가 필요합니다.
macOS에서는 Xcode Command Line Tools를 설치하세요. 이 repo의 Cargo config와 scripts는 Apple target에서
`/usr/bin/clang`을 우선 사용합니다. GitHub release의 prebuilt binary는 로컬 compiler가 필요하지 않습니다.

자세한 내용은 [install / uninstall docs](install-uninstall.md)를 참고하세요.

## Shell 통합

```sh
eval "$(yuru --bash)"
source <(yuru --zsh)
yuru --fish | source
```

PowerShell:

```powershell
yuru --powershell | Invoke-Expression
```

지원되는 키:

- `CTRL-T`: 파일 또는 디렉터리를 선택해 명령줄에 삽입
- `CTRL-R`: 명령 기록 검색
- `ALT-C`: 선택한 디렉터리로 이동
- `**<TAB>`: fuzzy path completion

## 사용 예시

중국어 병음 초성:

```sh
printf "北京大学.txt\nnotes.txt\n" | yuru --lang zh --filter bjdx
```

일본어 romaji:

```sh
printf "カメラ.txt\ntests/日本人の.txt\n" | yuru --lang ja --filter kamera
```

한국어 한글 romanization / 초성 / 2벌식 keyboard:

```sh
printf "한글.txt\nnotes.txt\n" | yuru --lang ko --filter hangeul
printf "한글.txt\nnotes.txt\n" | yuru --lang ko --filter ㅎㄱ
printf "한글.txt\nnotes.txt\n" | yuru --lang ko --filter gksrmf
```

파일 검색:

```sh
fd --hidden --exclude .git . | yuru --scheme path
```

## fzf 호환성과 설정

Yuru는 fzf의 주요 option surface를 parse할 수 있어 기존 shell binding과 `FZF_DEFAULT_OPTS`가 parse error로 멈출 가능성을 줄였습니다. `--filter`, `--query`, `--read0`, `--print0`, `--nth`, `--with-nth`, `--scheme`, `--walker`, `--expect`는 구현되어 있습니다. `--bind`는 subset 지원이며, 아직 지원하지 않는 action은 기본적으로 warning을 출력합니다.

```sh
yuru --fzf-compat warn
yuru --fzf-compat strict
yuru --fzf-compat ignore
```

preview command가 이미지 bytes를 출력하면 `ratatui-image`로 렌더링합니다. 필요한 경우
`YURU_PREVIEW_IMAGE_PROTOCOL=sixel|kitty|iterm2|halfblocks`로 protocol을 고정할 수 있습니다.
이미지 preview는 기본으로 켜진 `image` feature로 제공됩니다. 더 작은 source build가
필요하면 `cargo install yuru --no-default-features`를 사용할 수 있습니다.

`~/.config/yuru/config.toml`에서 `lang = "auto"`, `load_fzf_defaults = "safe"`, `algo = "greedy" | "fzf-v1" | "fzf-v2" | "nucleo"`, `[ja] reading = "none" | "lindera"`, `[ko] initials = true`, `[zh] initials = true` 등을 설정할 수 있습니다. CLI 인자가 가장 높은 우선순위를 가집니다.

자세한 호환성은 [fzf compatibility](fzf-compat.md), 언어 매칭 동작은 [language matching](language-matching.md)를 참고하세요.

## 개발

```sh
./scripts/install-hooks
./scripts/check
./scripts/bench
YURU_BENCH_1M=1 ./scripts/bench
```

git hook은 formatter, linter, test, benchmark를 실행합니다. 로컬에서 임시로 빠른 체크포인트가 필요할 때만
`YURU_SKIP_BENCH=1`을 사용하세요.

## 릴리스

version tag 를 push 하면 GitHub Actions 가 macOS, Linux, Windows용 release asset을 생성하고 crates.io에 publish합니다.
release workflow 는 tag push 에서만 실행되며, tag 는 crate version 과 일치해야 합니다.

```sh
git tag v0.1.8
git push origin v0.1.8
```

## 라이선스

Yuru는 MIT license와 Apache License 2.0의 조건으로 배포됩니다.
[LICENSE-MIT](../LICENSE-MIT) 및 [LICENSE-APACHE](../LICENSE-APACHE)를 참고하세요.
