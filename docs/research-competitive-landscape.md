# Research: Commercial & Open-Source Data Removal Landscape

*Compiled 2026-07-20 from web research. Informs ODR's architecture and positioning.*

## How the commercial services work

Three models:

1. **Fully automated** (Incogni, Optery, Kanary, EasyOptOuts) — templated CCPA/GDPR deletion emails and scripted form submissions against a pre-built broker list. Incogni covers ~420 brokers at ~$7/mo, resubmitting every 60–90 days. Optery covers ~950 at $8–13/mo with continuous monitoring. EasyOptOuts is a pure bot covering ~200 sites for $19.99/year.
2. **Human-operated** (DeleteMe) — privacy specialists manually work broker forms, quarterly rescans, ~$11/mo. Users hand a full personal dossier to human operators.
3. **User-guided / open source** (BADBOOL, DataPurge, Eraser) — free tools where the user submits requests themselves; data stays local.

## Consumer Reports 2024 effectiveness study (the key finding)

32 volunteers, 7 services, 13 people-search sites, 4 months:

- Overall, **only 35% of identified personal data stayed removed** after 4 months across paid services.
- **Manual opt-out by the user scored 70%** — the best result of anything tested — but took ~20–25 hours of effort.
- Optery: 68% (best paid). EasyOptOuts: 65% (best value). **DeleteMe: 27%** — the human-operated, most famous service was the *worst* tested.
- Sources: https://advocacy.consumerreports.org/press_release/consumer-reports-evaluation-of-people-search-site-removal-services-finds-that-they-are-largely-ineffective/

Implication: the assisted-manual model (automate the tedium, keep the human in the loop) isn't a compromise — it's the *most effective known approach*.

## Why removals don't stick (repopulation)

- Brokers re-scrape public records and buy commercial datasets every 30–90 days; Spokeo/BeenVerified refresh every 2–4 weeks.
- Opt-outs suppress a record snapshot; they don't delete the sources. Data typically reappears in 3–6 months.
- Effective removal = **recurring cycle every ~60 days**, not a one-shot. This makes the state tracker / re-check scheduler a core feature, not a nice-to-have.

## Broker-side friction (measured, from arxiv studies)

- ~75% of brokers use web forms, ~30% accept email; zero standardization. (arxiv.org/html/2607.04552)
- **70% of brokers failed to respond** to deletion requests at all; 22% illegally demand ID verification for opt-outs (CCPA prohibits this).
- CAPTCHA: 100% of OneTrust-hosted forms, ~37% of others. Some CAPTCHAs render invisibly and silently break automated submission.
- Email confirmation links expire in 24–48h — the tool must surface these to the user fast.
- **Authorized-agent discrimination**: brokers detect removal-service IP ranges and route them to slow queues — agent requests take 45–90 days vs 2–14 days for first-party requests. Big argument for local-first: requests from the user's own machine/IP/email are first-party.

## Legal landscape

- **California Delete Act (SB 362) / DROP platform**: consumer access launched Jan 1 2026; broker compliance deadline Aug 1 2026. One request hits all registered brokers; brokers must poll DROP every 45 days, delete within 90, $200/request/day penalties. ODR should integrate/point CA users at DROP rather than duplicate it.
- **CCPA**: opt-outs may not require identity verification (deletion requests may). 45-day response window. Authorized agents allowed but brokers can demand extra verification — except through DROP.

## Existing open-source tools (prior art to learn from / not duplicate)

- **BADBOOL** (github.com/yaelwrites/Big-Ass-Data-Broker-Opt-Out-List) — curated priority-tiered opt-out list, ~50 core brokers, community-maintained. Best seed dataset for our recipes.
- **Eraser** (github.com/digisamroc/eraser) — bulk removal emails to 750+ brokers, tracks responses. No monitoring loop.
- **DataPurge** (github.com/puurpl/datapurge) — generates legally-cited deletion emails (CCPA/GDPR/state laws), browser-local storage, MIT.
- **Permission Slip** (Consumer Reports) — authorized-agent model; being transferred to DeleteMe management in 2026.

Gap none of them fill: **the recurring loop** — scan → remove → verify → re-check on a schedule, with per-broker state. That plus browser-driven form automation is ODR's niche.

## Design implications for ODR

1. **Local-first is validated twice over**: avoids the centralized-dossier attack surface, *and* dodges authorized-agent discrimination since requests come from the user's own machine and email.
2. **Human-in-the-loop automation targets the 70% result**: drive the browser, prefill the forms, let the user click through CAPTCHAs and confirmation emails. Sell it as "manual opt-out effectiveness at 1/20th the effort."
3. **Recurrence is the product**: per-broker state machine (requested → awaiting-confirmation → confirmed → recheck-due), verify mode that re-searches brokers, 60-day default re-check cadence.
4. **Curate, don't sprawl**: 50 high-priority brokers with *working, tested* recipes beats 900 stale ones. Tier recipes like BADBOOL does.
5. **Surface confirmation emails urgently** (24–48h expiry) — consider an inbox-watch helper or at least loud prompts.
6. **Document non-compliance**: track no-response brokers and generate regulator complaint templates (CPPA/FTC/state AGs) — that's the enforcement lever no paid service offers.
7. **DROP integration** for California users once the Aug 2026 compliance deadline passes.

## Honest limitations to state up front

- No one-shot removal sticks; repopulation is structural.
- Some brokers can't be automated (ID upload, fax, phone) — guide, don't fake it.
- Never attempt CAPTCHA defeat — user solves them in the visible browser.
