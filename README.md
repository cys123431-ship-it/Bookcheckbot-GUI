# Bookcheckbot

**Windows 11 / Rust / Cargo** 기반의 전자책 뷰어 화면 자동 캡처 프로그램입니다. **창(GUI)에서 설정하고 버튼 한 번으로 실행**하거나, 예전처럼 명령줄(CLI) 인자로 실행할 수 있습니다. 사용자가 화면에서 볼 수 있고 캡처가 허용된 콘텐츠만 페이지별 PNG로 저장합니다. 브라우저/뷰어의 화면을 일정 간격으로 캡처한 뒤 키보드 또는 마우스로 다음 페이지를 넘깁니다.

> **사용 조건**: 사용자가 적법하게 접근할 수 있고 화면 캡처가 허용된 자료의 개인 열람/백업에만 사용하세요. 콘텐츠에 대한 권리를 확인해야 합니다. DRM, 스크린샷 차단, 접근 제어를 우회하지 않습니다. 재배포/공유/판매 기능도 없습니다. 결과 이미지가 완전히 검거나 하얗게 나오면 차단 가능성이 있으므로 우회하지 말고 중단하세요.

## 준비

1. Windows 11에 [Rust / rustup](https://rustup.rs/)을 설치하고 터미널을 다시 엽니다.
2. 터미널에서 `rustc --version`, `cargo --version`을 확인합니다. Rust 설치 시 MSVC C++ 빌드 도구 설치 안내가 나오면 따르세요.
3. 저장소를 복제하고 디렉터리로 이동합니다.

```powershell
git clone https://github.com/cys123431-ship-it/Bookcheckbot-GUI.git
cd Bookcheckbot-GUI
cargo build --release
cargo test
```

별도 IDE나 Visual Studio F5는 필요하지 않습니다. 처음 빌드할 때는 GUI 라이브러리(eframe/egui)를 포함한 의존성을 내려받고 컴파일하므로 몇 분 정도 걸릴 수 있습니다.

## 실행 방법 1: 화면(GUI) — 가장 간단

```powershell
cargo run --release
```

빌드된 실행 파일을 직접 실행해도 됩니다. (아래 `Bookcheckbot.exe`는 `cargo build --release` 결과물을 복사해 둔 것입니다.)

```powershell
.\Bookcheckbot.exe
```

인자 없이 실행하면 **캡처 설정 창이 바로 뜹니다.** 창에서 다음 항목을 입력합니다.

| 항목 | 설명 |
| --- | --- |
| 마지막 페이지 | 책의 총 페이지 수(예: 458) |
| 시작 페이지 | 뷰어에 지금 표시된 페이지(예: 1) |
| 저장 폴더 | PNG를 저장할 폴더(없으면 자동 생성) |
| 모니터 | 캡처할 모니터 선택 |
| 캡처 영역 직접 지정 | 체크하면 X / Y / 너비 / 높이 입력, 끄면 모니터 전체 |
| 넘김 방식 | 키보드(→ Right / PageDown / Space) 또는 마우스 클릭(X, Y) |
| 대기 시간 | 시작 카운트다운(초), 캡처 전 대기(ms), 넘김 후 대기(ms) |

[캡처 시작]을 누르면 카운트다운 동안 전자책 뷰어를 클릭해 포커스를 맞추면 되고, 진행 상황(`2/458 페이지 저장 중...`)이 창에 표시됩니다. 멈추려면 [중단] 버튼을 누릅니다. 캡처는 별도 스레드에서 실행되므로 창이 멈추지 않습니다. GUI에서 입력한 값은 CLI와 **동일한 검증 로직**을 거칩니다.

> 창이 뜨지 않고 "GUI 창을 열지 못했습니다" 오류가 보이면 그래픽 드라이버를 최신으로 업데이트하세요. 이 프로그램은 내장 그래픽/저사양 PC에서도 동작하도록 OpenGL(glow) 렌더러를 사용하며, 한글은 Windows 기본 글꼴(맑은 고딕)을 읽어 표시합니다.

## 실행 방법 2: 명령줄(CLI)

`--pages`를 지정하면 GUI 없이 바로 캡처합니다. 아래 명령은 **저장소 루트에서** PowerShell 또는 Windows Terminal에 입력하세요. `cargo run --release --` 뒤에 CLI 인자가 옵니다.

**첫 테스트: 3페이지** (좌표는 예시이므로 자신의 화면에 맞게 바꿉니다.)

```powershell
cargo run --release -- --pages 3 --out test --x 210 --y 130 --w 1240 --h 970 --turn key --key right
```

**458페이지 전체 캡처:**

```powershell
cargo run --release -- --pages 458 --out book1 --x 210 --y 130 --w 1240 --h 970 --turn key --key right
```

**151페이지부터 이어찍기** (뷰어를 먼저 151페이지로 직접 이동합니다.)

```powershell
cargo run --release -- --pages 458 --start 151 --out book1 --x 210 --y 130 --w 1240 --h 970 --turn key --key right
```

**화면 오른쪽 화살표를 마우스로 클릭해 페이지 넘기기:**

```powershell
cargo run --release -- --pages 458 --out book1 --x 210 --y 130 --w 1240 --h 970 --turn click --click-x 1750 --click-y 950
```

**선택 모니터 전체 캡처:**

```powershell
cargo run --release -- --pages 458 --out book1 --turn key --key right
```

**두 번째 모니터 선택 + 렌더링 기다리기:**

```powershell
cargo run --release -- --pages 3 --screen 1 --out test2 --turn key --key pagedown --render-wait 1400 --turn-wait 1500
```

3페이지 테스트가 끝나면 `test/page_0001.png`, `test/page_0002.png`, `test/page_0003.png`가 생성되어야 합니다. 페이지 번호는 4자리 이상일 때는 실제 번호만큼 확장됩니다. 책마다 총 페이지 수가 다르므로 `--pages`로 지정하세요.


## 옵션

| 옵션 | 기본값 | 설명 |
| --- | --- | --- |
| `--pages <u32>` | 없음 | 마지막 페이지 번호(1 이상). **없으면 GUI 창이 뜹니다.** |
| `--start <u32>` | 1 | 시작 페이지(뷰어 위치와 같아야 함) |
| `--out <path>` | shots | 저장 폴더(없으면 생성) |
| `--x <i32> --y <i32> --w <u32> --h <u32>` | 생략 | 지정 영역 캡처. **네 옵션을 전부 입력**해야 함. x/y는 Windows 전체 화면의 절대 좌표 |
| `--screen <usize>` | 0 | 모니터 인덱스(0부터) |
| `--turn <key\|click>` | key | 페이지 넘김 방식 |
| `--key <right\|pagedown\|space>` | right | 키보드 넘김 키 |
| `--click-x <i32> --click-y <i32>` | 없음 | click 방식의 절대 좌표 |
| `--start-delay <sec>` | 5 | 시작 전 뷰어에 포커스를 맞출 시간(초) |
| `--render-wait <ms>` | 700 | 각 페이지 캡처 전 대기(ms) |
| `--turn-wait <ms>` | 800 | 페이지 넘김 후 대기(ms) |

`--help`로 인자 안내를 볼 수 있습니다.

## 좌표 확인 (PowerShell)

Windows PowerShell에서 아래 명령을 실행하고 책 페이지 왼쪽 위 / 오른쪽 아래에 마우스를 각각 올려 좌표를 읽으세요.

```powershell
Add-Type -AssemblyName System.Windows.Forms
while ($true) {
    $p = [System.Windows.Forms.Cursor]::Position
    "$($p.X), $($p.Y)"
    Start-Sleep -Milliseconds 500
}
```

PowerShell 창에서 `Ctrl+C`로 종료합니다.

예를 들어 왼쪽 위가 **(210, 130)**, 오른쪽 아래가 **(1450, 1100)**이면 다음과 같이 계산합니다.

- `x=210`, `y=130`
- `w=1450-210=1240`
- `h=1100-130=970`

즉 `--x 210 --y 130 --w 1240 --h 970`입니다. 여러 모니터일 때 왼쪽에 있는 모니터의 X 값이 음수일 수 있습니다. 프로그램은 Windows 전체 가상 화면의 절대 좌표를 지정된 모니터의 상대 좌표로 변환합니다. **캡처 영역은 지정한 모니터 한 대 안에 모두 포함되어야** 하며, 넘어서면 에러로 중단합니다.

## 작동 방식과 주의사항

1. 브라우저의 전자책 뷰어를 열고 **시작 페이지를 직접 표시**하세요. 이 프로그램은 뷰어가 실제로 몇 페이지인지 읽거나 보정하지 않습니다.
2. 뷰어의 창 크기, 브라우저 확대 비율, Windows 디스플레이 배율을 고정하세요. 처음에는 **배율 100%**가 안전합니다.
3. 명령을 실행한 뒤 기본 5초 카운트다운 동안 뷰어를 클릭해 **포커스를 맞추세요**.
4. 프로그램이 PNG를 저장하고 마지막 페이지를 제외한 각 페이지에서 다음 페이지로 넘깁니다. 마지막 페이지에서는 **추가 입력을 보내지 않습니다**.
5. 진행 중 키보드와 마우스를 건드리지 말고, 필요하면 터미널에서 `Ctrl+C`로 중단하세요.
6. 중간에 중단했다면 뷰어를 원하는 페이지에 **직접 이동**하고 `--start`와 기존 `--out`을 지정해 이어찍습니다. 같은 이름의 PNG 파일은 덮어써질 수 있으므로 시작 번호를 확인하세요.

화면이 거의 모두 흰색/검정색인 경우 프로그램이 PNG를 저장한 뒤 오류를 표시하고 중단합니다. 실제로 내용이 없는 페이지인 경우에도 이 검사가 작동할 수 있습니다. 저장된 이미지를 확인하고, 로딩 지연이라면 `--render-wait`나 `--turn-wait`를 늘려 다시 실행하세요. **캡처 제한이 원인이라면 우회하지 마세요.**

모니터 인덱스는 연결 상태나 배열에 따라 바뀔 수 있습니다. 우선 `--pages 3`으로 시도해 원하는 모니터가 캡처되는지 확인하세요. 고해상도/배율이 서로 다른 모니터에서는 좌표 및 DPI를 재확인하세요.

## 자주 발생하는 문제

| 현상 | 해결 |
| --- | --- |
| `cargo`가 인식되지 않음 | rustup 설치 후 터미널을 재시작하고 `cargo --version` 확인 |
| C++/링커 설치 오류 | Windows Rust MSVC 대상 및 Visual Studio Build Tools의 C++ 도구 설치 |
| `--pages 1 --start 2` 실패 | 정상: 시작 번호가 마지막 번호보다 클 수 없음 |
| `--x`만 입력하여 실패 | `--x --y --w --h` 네 값을 모두 지정 |
| `--turn click` 실패 | `--click-x --click-y` 둘 다 지정 |
| 모니터 번호가 잘못되었다는 오류 | 실제 연결된 모니터 인덱스(0부터)로 재지정 |
| 캡처 영역이 모니터 밖이라고 나옴 | 선택한 모니터의 위치·크기와 좌표를 확인 |
| 페이지가 안 넘어감 | 뷰어에 포커스를 맞추고 적합한 키(`right`, `pagedown`, `space`) 또는 클릭 좌표로 다시 시험 |
| 일부 이미지가 중복되거나 미완성 | 뷰어 속도에 맞춰 `--render-wait`, `--turn-wait`를 늘림 |
| 이미지가 검거나 희거나 차단됨 | 저장본을 확인. 캡처 제한이면 우회하지 말고 중단 |
| `--start`로 이어서 찍는데 페이지가 다름 | 프로그램은 뷰어 페이지를 자동 이동하지 않으므로 수동으로 정확한 페이지로 이동 |
| 저장 파일을 덮어씀 | 해당 이름의 파일이 없는 폴더/시작 번호 선택 |

## 테스트

```powershell
cargo test
cargo build --release
cargo run --release -- --help
```

Rust 단위 테스트는 페이지 범위 검증, 부분 영역 거부, 클릭 좌표 필수 검사, 음수 모니터 좌표 변환, 3페이지/2페이지 시퀀스, 마지막 페이지에서 넘기지 않는 동작, GUI 입력값 검증을 확인합니다. 실제 화면 입력/PNG 생성은 **Windows 11에서 3페이지 시험 실행으로 별도 확인**해야 합니다. GitHub Actions는 Windows에서 빌드와 단위 테스트를 실행합니다.

## 프로젝트 구성

```text
Bookcheckbot-GUI/
├── Cargo.toml
├── src/
│   ├── main.rs      # 진입점: 인자가 없으면 GUI, --pages가 있으면 CLI
│   ├── cli.rs       # 명령줄 옵션 파싱·검증
│   ├── capture.rs   # 실제 캡처/페이지 넘김 엔진(CLI·GUI 공용)
│   └── gui.rs       # egui/eframe 설정 창
├── .github/
│   └── workflows/
│       └── windows.yml
├── .gitignore
├── RUN_WINDOWS.md
└── README.md
```

프로그램 자체는 PNG 파일만 만듭니다. **PDF 통합 기능은 기본 동작에 넣지 않았습니다.** 캡처한 책 내용이나 PNG 파일은 Git에 커밋하지 마세요(저장소 `.gitignore`가 PNG·캡처 폴더·빌드 산출물을 제외합니다).

