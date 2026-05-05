# Yomi

Yomi는 일본어 읽기 검색과 중국어 병음 검색을 지원하는 빠른 명령줄 fuzzy finder입니다.
fzf와 비슷한 사용감을 유지하면서 CJK 텍스트의 phonetic match와 정확한 하이라이트를 목표로 합니다.

## 설치

Yomi는 기본적으로 사용자 영역에 설치됩니다. `sudo`가 필요하지 않습니다.

macOS / Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yomi/main/install | sh -s -- --all
```

기본 설치 위치는 `~/.local/bin`입니다. `XDG_BIN_HOME` 또는 `YOMI_INSTALL_BIN_DIR`로 변경할 수 있습니다.
`--all`을 사용하면 현재 shell 설정에 통합 스크립트도 추가합니다.

Windows PowerShell:

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yomi/main/install.ps1
Invoke-Expression "& { $script } -All"
```

`%LOCALAPPDATA%\Yomi\bin`에 `yomi.exe`를 설치하고, 사용자 PATH와 PowerShell profile을 업데이트합니다.

바이너리만 설치:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yomi/main/install | sh
```

## Shell 통합

```sh
eval "$(yomi --bash)"
source <(yomi --zsh)
yomi --fish | source
```

PowerShell:

```powershell
yomi --powershell | Invoke-Expression
```

지원되는 키:

- `CTRL-T`: 파일 또는 디렉터리를 선택해 명령줄에 삽입
- `CTRL-R`: 명령 기록 검색
- `ALT-C`: 선택한 디렉터리로 이동
- `**<TAB>`: fuzzy path completion

## 사용 예시

중국어 병음 초성:

```sh
printf "北京大学.txt\nnotes.txt\n" | yomi --lang zh --filter bjdx
```

일본어 romaji:

```sh
printf "カメラ.txt\ntests/日本人の.txt\n" | yomi --lang ja --filter kamera
```

파일 검색:

```sh
yomi --walker file,dir,follow,hidden --scheme path
```

## 개발

```sh
./scripts/install-hooks
./scripts/check
./scripts/bench
YOMI_BENCH_1M=1 ./scripts/bench
```

git hook은 formatter, linter, test, benchmark를 실행합니다. 로컬에서 임시로 빠른 체크포인트가 필요할 때만
`YOMI_SKIP_BENCH=1`을 사용하세요.

## 릴리스

tag를 push하면 GitHub Actions가 macOS, Linux, Windows용 release asset을 생성합니다.

```sh
git tag v0.1.0
git push origin v0.1.0
```
