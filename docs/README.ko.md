# Yuru

Yuru는 일본어 읽기, 한국어 한글, 중국어 병음으로 검색할 수 있는 빠른 명령줄 fuzzy finder입니다.
fzf와 비슷한 사용감을 유지하면서 CJK 텍스트를 발음 기반으로 찾고, 원문을 정확히 하이라이트하는 데 초점을 둡니다.

## 데모 영상

[Yuru 명령 데모 보기](../demo.mp4)

<video src="../demo.mp4" controls muted playsinline width="100%"></video>

## 설치

Yuru는 기본적으로 사용자 영역에 설치되므로 `sudo`가 필요하지 않습니다.

macOS / Linux 대화형 설치:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.10/install | sh -s -- --all --version v0.1.10
```

기본 설치 위치는 `~/.local/bin`입니다. `XDG_BIN_HOME` 또는 `YURU_INSTALL_BIN_DIR`로 변경할 수 있습니다.
이 명령을 대화형 터미널에서 실행하면 기본 언어, 미리보기 명령, 텍스트 미리보기 확장자,
이미지 미리보기 프로토콜, 셸 바인딩, 셸 경로 검색 백엔드를 묻고
`~/.config/yuru/config.toml`에 저장합니다. 각 프롬프트에서 Enter를 누르면 해당 항목의 기본값을 사용합니다.
미리보기 명령 기본값 `auto`는 텍스트 미리보기에서 사용할 수 있으면 `bat`을 쓰고, 이미지는 Yuru 내장 미리보기를 사용합니다.
이미지 미리보기 프로토콜 기본값은 `none`입니다. 셸 경로 검색 백엔드 기본값 `auto`는
`fd`, `fdfind`, 이식 가능한 대체 경로 순서로 시도합니다.

한국어를 기본 검색 언어로 설정하고 대화형 설치의 값을 명시하려면:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.10/install | sh -s -- --all --version v0.1.10 --default-lang ko --preview-command auto --preview-image-protocol none --path-backend auto --bindings all
```

나중에 설정을 바꾸려면 `yuru configure`를 실행합니다.

Windows PowerShell:

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.10/install.ps1
Invoke-Expression "& { $script } -All -Version v0.1.10"
```

`%LOCALAPPDATA%\Yuru\bin`에 `yuru.exe`를 설치하고, 사용자 PATH와 PowerShell profile을 업데이트합니다.
대화형 환경에서는 기본 언어, 미리보기 명령, 텍스트 미리보기 확장자, 이미지 미리보기 프로토콜, 셸 바인딩, 셸 경로 검색 백엔드를 묻습니다.
한국어를 기본 언어로 설정하려면 다음처럼 명시합니다.

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.10/install.ps1
Invoke-Expression "& { $script } -All -Version v0.1.10 -DefaultLang ko -PreviewCommand auto -PreviewImageProtocol none -PathBackend auto -Bindings all"
```

바이너리만 설치:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.10/install | sh -s -- --version v0.1.10
```

crates.io에서 설치:

```sh
cargo install yuru
```

crates.io 패키지 이름과 설치되는 명령은 모두 `yuru`입니다.
소스에서 빌드할 때는 일본어 읽기를 위해 Lindera embedded IPADIC을 사용하므로 C 컴파일러가 필요합니다.
macOS에서는 Xcode Command Line Tools를 설치하세요. 이 저장소의 Cargo config와 scripts는 Apple target에서
`/usr/bin/clang`을 우선 사용합니다. GitHub release의 미리 빌드된 바이너리는 로컬 컴파일러가 필요하지 않습니다.

자세한 내용은 [install / uninstall docs](install-uninstall.md)를 참고하세요.

## 셸 통합

```sh
eval "$(yuru --bash)"
source <(yuru --zsh)
yuru --fish | source
```

PowerShell:

```powershell
Invoke-Expression ((yuru --powershell) -join "`n")
```

지원되는 동작:

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

한국어 Hangul romanization / 초성 / 2벌식 keyboard:

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

Yuru는 fzf의 주요 옵션 묶음을 해석할 수 있어 기존 셸 바인딩과 `FZF_DEFAULT_OPTS`가 구문 분석 오류로 멈출 가능성을 줄입니다.
`--filter`, `--query`, `--read0`, `--print0`, `--nth`, `--with-nth`, `--scheme`, `--walker`, `--expect`는 구현되어 있습니다.
`--bind`는 일부만 지원하며, 아직 지원하지 않는 동작은 기본적으로 경고를 출력합니다.

```sh
yuru --fzf-compat warn
yuru --fzf-compat strict
yuru --fzf-compat ignore
```

미리보기 명령이 이미지 바이트 데이터를 출력하면 `ratatui-image`로 렌더링합니다. 필요한 경우
`YURU_PREVIEW_IMAGE_PROTOCOL=sixel|kitty|iterm2|halfblocks`로 프로토콜을 고정할 수 있습니다.
이미지 미리보기는 기본으로 켜진 `image` feature로 제공됩니다. 더 작은 소스 빌드가
필요하면 `cargo install yuru --no-default-features`를 사용할 수 있습니다.

한국어를 기본으로 사용하려면 `~/.config/yuru/config.toml`에서 `lang = "ko"`를 설정합니다.
하나의 후보 목록에서 일본어, 한국어, 중국어 검색을 함께 지원하려면 `lang = "all"`을 사용합니다.
그 밖에도 `lang = "auto"`, `load_fzf_defaults = "safe"`, `algo = "greedy" | "fzf-v1" | "fzf-v2" | "nucleo"`,
`[ja] reading = "none" | "lindera"`, `[ko] initials = true`, `[zh] initials = true` 등을 설정할 수 있습니다.
CLI 인자가 가장 높은 우선순위를 가집니다.

자세한 호환성은 [fzf compatibility](fzf-compat.md), 언어 매칭 동작은 [language matching](language-matching.md)를 참고하세요.

## 개발

```sh
./scripts/install-hooks
./scripts/check
./scripts/bench
YURU_BENCH_1M=1 ./scripts/bench
```

git hook은 formatter, linter, test, benchmark를 실행합니다. 로컬에서 임시로 benchmark를 건너뛰어야 할 때만
`YURU_SKIP_BENCH=1`을 사용하세요.

## 릴리스

version tag를 푸시하면 GitHub Actions가 macOS, Linux, Windows용 릴리스 파일을 만들고 crates.io에 게시합니다.
release workflow는 tag push에서만 실행되며, tag는 crate version과 일치해야 합니다.

```sh
git tag v0.1.10
git push origin v0.1.10
```

## 라이선스

Yuru는 MIT 라이선스와 Apache License 2.0의 조건으로 배포됩니다.
[LICENSE-MIT](../LICENSE-MIT) 및 [LICENSE-APACHE](../LICENSE-APACHE)를 참고하세요.
