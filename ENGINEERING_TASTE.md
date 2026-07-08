# ENGINEERING_TASTE.md

Purpose:

Approximate the owner's engineering taste.
When uncertain, optimize for predictability and minimal disruption.

## Core Philosophy

Prefer:

- Small incremental changes
- Boring, understandable solutions
- Shipping over theoretical elegance
- Explicit state over hidden magic
- Consistency over novelty

Avoid:

- Large rewrites
- Speculative abstractions
- Unrelated refactors
- New infrastructure without a proven pain point
- Silent behavior changes

---

## Decision Priority

When tradeoffs exist, prioritize in this order:

1. Preserve existing behavior
2. Keep scope small
3. Keep implementation simple
4. Reduce maintenance burden
5. Improve architecture
6. Improve performance

Do not sacrifice higher priorities for lower ones.

---

## Change Style

Prefer:

- Minimal patches
- Localized changes
- Existing patterns in the codebase
- Incremental migration over replacement

Avoid:

- Rewriting working systems during unrelated tasks
- Framework churn
- Premature generalization

Rule of thumb:

"If it already works, improve carefully."

---

## Path References

문서(`.md`)와 코드(`.rs`)에서 파일을 참조할 때:

- 항상 프로젝트 루트 기준 상대경로를 사용한다
- `D:\dev\...`, `C:\Users\...` 등 로컬 절대경로를 절대 사용하지 않는다

Good: `rust/overmax_engine/src/detector/play_state.rs`
Bad: `D:\dev\overmax\rust\overmax_engine\src\detector\play_state.rs`

---

## Scope Control

Only solve the requested problem.

Do not:

- Expand requirements
- Add future-proofing unless explicitly requested
- Introduce unrelated cleanup
- Refactor adjacent systems "while here"

Good:

- Small fix for the requested task

Bad:

- Turning a small task into architecture work

---

## Communication Style

Work in short loops.

Prefer:

- Small deliverables
- Frequent checkpoints
- Showing diffs before large changes
- Explaining tradeoffs briefly

When uncertain:

Ask or stop and report.

Never make large architectural decisions silently.

---

## Decision Heuristics

When uncertain:

Prefer minimal change.

When two options are similar:

Choose the more boring solution.

Before adding dependency/infrastructure:

Show the concrete pain point first.

Before refactoring:

Explain why existing structure is insufficient.

When tempted to improve unrelated code:

Do not.

---

## Red Flags (정지 후 사용자 확인 트리거)

다음과 같이 위험도가 높거나 영향 범위가 큰 변경을 시도하기 전에는 반드시 작업을 멈추고 사용자에게 명시적으로 확인을 구해야 한다. (단순 리팩토링이나 국소 수정 시 과도한 정지를 방지하기 위해 트리거를 좁힘)

- **핵심 모듈 교체**: 작동 중인 기존 핵심 파이프라인(예: 캡처, 매칭 엔진)을 다른 라이브러리나 다른 아키텍처로 완전히 대체할 때
- **의존성 및 인프라 추가**: 새로운 외부 데이터베이스, 서비스, 프레임워크 또는 무거운 외부 크레이트(dependency)를 추가할 때
- **빌드/릴리스 시스템 변경**: 빌드 스크립트(`build.bat`)나 릴리스 파이프라인의 구조적 변경
- **광범위한 아키텍처 변경**: 3개 이상의 서로 다른 크레이트(Crate)에 걸쳐 핵심 데이터 모델(예: `PlayContext`, `GameSessionState`)을 수정하거나 설계를 변경할 때

---

## Success Criteria

Good work feels:

- Predictable
- Small in scope
- Easy to review
- Easy to revert
- Easy to maintain
