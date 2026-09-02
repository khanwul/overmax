# Microsoft Store Listing & Policy Compliance Guide

Overmax의 Microsoft Store(Windows App Store) 등록, 심사 통과(Certification), 정책 준수 및 메타데이터 작성 가이드입니다.

---

## 1. 파트너 센터(Partner Center) 등록 절차

1. **Microsoft Partner Center 개발자 계정 로그인**:
   - [Microsoft Partner Center](https://partner.microsoft.com/dashboard)에 로그인합니다.
2. **새 앱 예약(Reserve app name)**:
   - 앱 이름을 `Overmax`로 예약합니다.
   - 예약 완료 후 **제품 관리(Product management) ➔ 제품 ID(Product Identity)** 메뉴에서 아래 식별자들을 확인합니다:
     - **패키지 패밀리 이름 (Package Family Name)**
     - **패키지 ID 이름 (Package/Identity Name)** (예: `xxxx.Overmax`)
     - **게시자 ID (Publisher ID)** (예: `CN=xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`)
     - **게시자 표시 이름 (Publisher Display Name)** (예: `Orphera`)
3. **릴리즈 패키지 생성**:
   - 발급된 식별자를 인자로 전달하여 공식 Store용 MSIX 패키지를 빌드합니다:
     ```powershell
     .\scripts\package-msix.ps1 -PackageName "<Your-Package-Name>" -Publisher "<Your-Publisher-ID>" -PublisherDisplayName "<Your-PublisherDisplayName>"
     ```

---

## 2. 스토어 등록 메타데이터 (Store Listings)

### 🇺🇸 English (United States)

- **App Name**: `Overmax`
- **Short Description**:
  Real-time companion overlay and intelligent recommendation utility for DJMAX RESPECT V players.
- **Description**:
  Overmax is a lightweight, non-invasive companion utility designed specifically for DJMAX RESPECT V players. 
  By seamlessly monitoring the in-game screen via native Windows Graphics Capture, Overmax provides real-time song identification, personalized difficulty recommendations based on your recent performance, and streamlined record tracking.

  **Key Features**:
  - **Zero In-Game Overhead**: Utilizes pure screen capture without process injection or memory tampering, ensuring maximum FPS stability and zero anti-cheat conflict.
  - **Intelligent Song Recommendations**: Tailors recommendation floors dynamically by analyzing current play trends, target skill zones, and player score history.
  - **Non-Obtrusive Overlay**: Sleek semi-transparent HUD and ultra-compact Lite Mode that stays out of your note lanes.
  - **Local Privacy**: All detection, scores, and caches are processed strictly on your local PC.
  - **Open Protocol & Extensibility**: Provides local IPC streaming and RPC endpoints for streaming setups and custom tools.

- **Feature Highlights**:
  - Real-time song and difficulty detection via lightweight computer vision
  - Trend-based smart song recommendations
  - Compact in-game overlay with Lite HUD mode
  - Safe, non-invasive screen capture (no DLL injection, no memory modification)
  - Seamless local score tracking and V-Archive synchronization

- **Keywords**:
  `DJMAX`, `DJMAX RESPECT V`, `Rhythm Game`, `Overlay`, `Song Recommendation`, `V-Archive`, `Companion`

---

### 🇰🇷 Korean (한국어)

- **앱 이름**: `Overmax`
- **간단한 설명**:
  DJMAX RESPECT V 플레이어를 위한 실시간 화면 인식 기반 인게임 오버레이 및 지능형 선곡 추천 유틸리티
- **상세 설명**:
  Overmax는 DJMAX RESPECT V 플레이어를 위해 정밀하게 설계된 경량 보조 유틸리티입니다.
  Windows 표준 그래픽 캡처를 통해 게임 화면을 안전하게 분석하여, 현재 곡과 난이도를 실시간으로 인식하고 플레이어의 실력대에 최적화된 맞춤형 추천 곡을 즉시 제안합니다.

  **주요 기능**:
  - **게임 성능 영향 최소화**: 프로세스 메모리 변조나 DLL 인젝션 없이 순수 화면 캡처(Desktop Bridge Win32 Capture) 방식으로 동작하여 100% 안전하고 쾌적한 프레임을 보장합니다.
  - **지능형 선곡 추천 엔진**: 플레이어의 Top 50 기록과 최근 판정 추세를 종합 분석하여 안정적인 실력 향상을 위한 최적의 난이도/곡을 실시간으로 추천합니다.
  - **방해 없는 오버레이 UI**: 게임 플레이를 가리지 않는 깔끔한 반투명 HUD 및 최소형 라이트 모드(Lite Mode)를 지원합니다.
  - **철저한 로컬 데이터 보호**: 모든 인식 연산과 플레이 기록은 외부 서버가 아닌 사용자 PC 로컬 디렉터리에만 안전하게 보관됩니다.
  - **외부 도구 연동 확장성**: 방송 오버레이 및 외부 서드파티 툴과의 연동을 위한 로컬 IPC 스트리밍 및 RPC 인터페이스를 제공합니다.

- **주요 기능 목록**:
  - 경량 컴퓨터 비전 기반 실시간 곡/난이도/판정 인식
  - 플레이어 실력 맞춤형 스마트 선곡 추천
  - 인게임 방해를 최소화하는 라이트 모드 반투명 오버레이
  - 100% 안전한 비침투식 캡처 (메모리 접근 및 파일 변조 일체 없음)
  - V-Archive 기록 동기화 및 로컬 플레이 통계 추적

- **검색 키워드**:
  `디제이맥스`, `DJMAX`, `리스펙트`, `리듬게임`, `오버레이`, `선곡 추천`, `V-Archive`, `유틸리티`

---

## 3. 필수 정책 대응 및 면책 조항 (Mandatory Disclaimers)

스토어 심사 시 "설명(Description)" 하단 및 서드파티 고지 사항에 **반드시** 아래 문구를 포함해야 합니다.

### ⚠️ Trademark & Fair Use Disclaimer (면책 조항)

```text
[Notice / Disclaimer]
Overmax is an independent, open-source third-party companion utility developed by the rhythm game community.
Overmax is NOT an official product of, nor is it affiliated with, endorsed by, sponsored by, or associated with NEOWIZ Corp. or the DJMAX RESPECT V development team.
All game titles, trademarks, logos, and visual assets related to DJMAX RESPECT V are the intellectual property of NEOWIZ Corp.

[Technical Safety & Integrity]
Overmax strictly adheres to non-invasive software architecture:
1. It does NOT inject code or DLLs into the game process.
2. It does NOT read, write, hook, or tamper with game memory.
3. It does NOT modify any game files or network packets.
All functionality is achieved solely through external screen capture (Windows Graphics Capture API) and local image analysis.
```

---

## 4. 심사 제출 필수 양식 (Submission Checklist)

### 🛡️ 1. 제한된 기능(Restricted Capabilities) 소명: `runFullTrust`
Windows App Certification Kit(WACK) 심사 시 `runFullTrust` 권한에 대한 사유 작성 요구가 발생합니다:
> **제출 설명문(Notes for Certification)**:
> "Overmax is a desktop utility requiring full trust (`runFullTrust`) to utilize Windows Graphics Capture APIs and Win32 Desktop Bridge features for non-invasive game window tracking, global tray notifications, and low-latency local overlay rendering."

### 🔞 2. 연령 등급(IARC) 설문
- **앱 카테고리**: 유틸리티 및 도구 (Utilities & tools)
- **폭력/선정성/도박/욕설 여부**: 모두 **아니오(No)**
- **사용자 간 통신/채팅 기능**: **아니오(No)**
- **사용자 위치 정보 공유**: **아니오(No)**
- **예상 등급**: 전 연령 이용가 (IARC 3+ / PEGI 3 / ESRB Everyone)

### 🔒 3. 개인정보처리방침 (Privacy Policy)
- **개인정보처리방침 URL**: 
  `https://github.com/orphera/overmax/blob/main/docs/store/PRIVACY.md`
- **핵심 요약**:
  - Overmax는 사용자의 개인정보나 민감한 식별 데이터를 수집, 저장 또는 원격 전송하지 않습니다.
  - 모든 캡처 프레임과 인식된 점수/기록은 사용자의 로컬 머신(`%LOCALAPPDATA%\Overmax`)에서만 처리되고 저장됩니다.
  - V-Archive 기록 동기화 기능은 사용자가 자발적으로 입력한 API 키에 한하여 V-Archive 공식 엔드포인트와 직접 HTTPS 통신합니다.

---

## 5. 필수 그래픽 에셋 규격 요약

| 에셋 항목 | 규격 (Pixel) | 파일 경로 | 설명 |
| :--- | :--- | :--- | :--- |
| **Square 44x44** | 44×44 | `packaging/msix/Assets/Square44x44Logo.png` | 앱 목록 및 작업 표시줄 |
| **Square 150x150** | 150×150 | `packaging/msix/Assets/Square150x150Logo.png` | 시작 메뉴 기본 타일 |
| **Wide 310x150** | 310×150 | `packaging/msix/Assets/Wide310x150Logo.png` | 시작 메뉴 와이드 타일 |
| **Square 310x310** | 310×310 | `packaging/msix/Assets/Square310x310Logo.png` | 시작 메뉴 대형 타일 |
| **Store Logo** | 50×50 | `packaging/msix/Assets/StoreLogo.png` | 스토어 검색 목록 아이콘 |
| **Splash Screen** | 620×300 | `packaging/msix/Assets/SplashScreen.png` | 패키지 실행 초기 화면 |
| **스토어 스크린샷** | 1920×1080 | 스토어 콘솔에 직접 업로드 (최소 1장) | 인게임 오버레이/라이트 모드 화면 |