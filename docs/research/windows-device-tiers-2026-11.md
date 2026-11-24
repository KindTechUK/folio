# Device tiers for Windows laptops in UK charities and SMEs (Nov 2026)

> Recorded 2026-11-20. Web research: analyst and survey data, UK retailer listings, Microsoft and Intel/AMD
> documentation. Purpose: copy voice2text's device tiers (`ios/App/Recommendation.swift`: Apple
> Intelligence → cleanup, otherwise "words as heard plus dictionary") for Folio on Windows, sized for
> buyers with low device budgets. No UK charity or SME fleet census was found, so the **share
> estimates are judgement** built from the sources below. Where a figure comes from a vendor or news
> report rather than an analyst or survey, it says so.

## Summary

- **Most target machines have no NPU, and RAM is the real constraint.** Charities rely on cheap
  and refurbished laptops. Half of new Western European notebooks still have no NPU, and 8 GB
  machines are back because of the 2026 memory shortage. Folio's tiers should split on **RAM first** and
  treat an NPU as a bonus.
- **Copilot+ machines are a small minority of the target fleet** (estimated 3–8%). No business Copilot+
  laptop was found under £800 ex VAT. Build the OS-model path as an upgrade, never as a dependency.
- **The Windows counterpart of Apple Intelligence is changing.** Phi Silica is being replaced by **Aion
  Instruct** (retail January 2027, no unlock token). Target the `LanguageModel` API, not a model name.
  Its GPU path needs Developer Mode and an Insider build, so it's not viable for our users.
- **Benchmark on a ~£110 refurbished i5-8265U with 8 GB of single-channel RAM**, not on the CI runner or
  a Mac. It's the realistic floor, and it costs less than a day of engineering time.

## Proposed Folio tiers

Like v2t, the app checks the device at start-up, names the tier in Models & settings, and explains in
one sentence what this machine gets and why. Nothing is blocked without saying why.

| Tier | The machine | What Folio does there | Est. share of target fleet |
|---|---|---|---|
| **Essentials** | Any x64 CPU with 4+ cores, **8 GB**. Typically Intel 8th–13th gen or Ryzen 3000–7000 with no NPU, often single-channel RAM. Floor: Intel N150. | Transcribe files, record (if S1 passes on the floor machine), diarization, vocabulary. **No local LLM notes**: a 3–4B model alongside Teams and a browser doesn't fit in 8 GB. | Large. Most of the ~75–85% with no NPU, split between 8 and 16 GB (no data on the split) |
| **Standard** | **16 GB**, any CPU. Includes NPU-under-40-TOPS machines: Core Ultra 1xx/2xxU (11–13 TOPS), Ryzen 7040/8040/230 (10–16). | Everything above, plus **local LLM notes on CPU** (S2). Plus the **OpenVINO speed-up** on NPU or iGPU *if* the bench shows a gain. | The rest of the no-NPU group plus the 10–20% with a small NPU |
| **Copilot+** | 40+ TOPS NPU **and** 16 GB+ (Lunar Lake, Ryzen AI 300, Snapdragon X, Panther Lake) | Everything above, with notes drafted by the **Windows on-device model** (`LanguageModel`; Aion Instruct from Jan 2027): no 2–3 GB download and faster. Falls back to Standard's model if `GetReadyState` says not supported. | ~3–8% |

What a tier must never change: **the transcript text itself.** Every tier transcribes with the same
model and settings, so a draft doesn't depend on which laptop made it. Tiers change speed and which
extra features are available.

### Reference machines for the Windows bench

| Role | Machine | Approx. cost | Why |
|---|---|---|---|
| Floor | Refurbished Dell Latitude 3400, i5-8265U, 8 GB (1×8 GB), SSD | ~£110 (charity refurbisher) | The typical donated or refurbished charity laptop |
| Absolute floor | New Intel N150 laptop, 8 GB | ~£250–350 | Cheapest new machine; single-channel only, 4 E-cores |
| Typical new budget | Lenovo V15 G4 or ASUS ExpertBook P1, i5-13420H, 8 GB | £333–£476 ex VAT | What an SME buys new today. No NPU |
| Standard + NPU | ThinkPad E14 Gen 7, Core Ultra 5 225U, 16 GB | £727–£764 ex VAT | Tests the OpenVINO NPU path |
| Copilot+ | Consumer Snapdragon X1-26-100, 16 GB | £335–£700 inc VAT | ARM64. Check diarization dependencies build for it |

## Evidence

**What charities have**
- Charity Digital Skills Report 2026 (survey, 807 charities, Jul 2026): 24% say IT support and hardware are
  poor or non-existent (small charities 30%). 19% name equipment as a barrier. 33% need funding for IT and devices.
- Charity Digital Skills Report 2025 (survey, 672 charities): 24% poor or non-existent, 52% "fair".
  Respondent quote: "we rely on 'cheaper' laptops and refurbished technology… one laptop serving 10
  members of staff."
- A charity refurbisher's listings (Computers 4 Good, fetched 20 Nov 2026): every machine was pre-NPU,
  Intel 8th–13th gen with 8–16 GB, priced £110–£350.
- UK Windows versions (Statcounter, Oct 2026, web traffic, not a fleet survey): Windows 11 86%, Windows 10 14%.
- UK SME device age: no primary source found.

**What's being bought**
- Western Europe notebook distribution, Q3 2026 (CONTEXT, analyst, published 16 Nov 2026): 40+ TOPS 26%
  (up from 6% a year earlier), under-40 TOPS AI 21%, **non-AI 52%**. No UK split.
- Windows PCs with 40+ TOPS: 0.6% in 2024, 2.3% in Q1 2025 (IDC via PCWorld). PCs with any NPU:
  15.6% of 2024 shipments (Gartner).
- Global shipments fell 21% in Q3 2026 (Omdia, via news), which slows fleet turnover.
- 8 GB models are back across Dell, Lenovo, HP and Surface because of the DRAM shortage (Tom's
  Hardware, Jun 2026; Windows Latest, Aug 2026; news). An 8 GB machine can never be Copilot+.
- UK prices (retailer listings): i5-13420H with 8 GB costs £333–£476 ex VAT, with no NPU. Business laptops
  with an NPU start around £680–£760 for 16 GB, at 11–12 TOPS. Business Copilot+ laptops start around
  £1,134 (HP ProBook 4 G1q).

**The Windows on-device model**
- Microsoft Learn, Phi Silica page (updated 16 Nov 2026), on the replacement: "Phi Silica is being replaced by Aion Instruct". Unlike Phi Silica, it needs no unlock token. It reaches Insiders in December 2026; in January 2027 it reaches retail devices and Phi Silica is removed.
- The same page, on hardware: the model runs on Copilot+ NPUs, or on RTX 30+ or RX 9060+ GPUs with 6 GB+ of VRAM. The GPU path needs
  **Developer Mode**, an Insider Experimental build, and drivers straight from the GPU maker. It downloads
  several GB through Windows Update on first use.
- Since October 2026, output from the Windows AI text APIs carries an invisible AI-generated watermark.
  Check what that means for notes exported into legal records.

## Gotchas for engineering

1. **Memory budget.** On 8 GB with Teams and a browser open, Folio may get roughly 2–3 GB (our own
   estimate, not measured). Parakeet int8 (~500 MB on disk) plus diarization has to fit, and so does the
   live-recording memory growth S1 found (+230 MB over 2 h). Measure peak private bytes on the floor
   machine.
2. **Single-channel RAM** roughly halves memory bandwidth on budget 8 GB models. Expect a slower real-time
   factor than the CI runner suggests. This is our own inference; no benchmark was found.
3. **S-mode.** Some Windows 11 Home laptops only run Store apps. The cheap consumer Copilot+ models are
   Home edition. Our `-setup.exe` won't install in S-mode, so say so in the install guide.
4. **"AI PC" stickers** go on 11–16 TOPS chips. Detect the NPU's capability; don't trust marketing names.
5. **ARM64** (Snapdragon): the engine, diarization and installer need ARM64 builds, or the app runs under
   emulation.
6. **Packaging.** The Windows AI APIs require a packaged app with package identity (MSIX). That was
   documented for Phi Silica; whether it still applies to Aion Instruct is unverified. It is the main
   cost of the Copilot+ tier.

## Parked: supplier specs, pending conversations with charities

Supplier specs are parked until we've spoken to charities: no public spec or price lists were found
for the frameworks below. Questions to ask, to place each organisation in a tier:

1. Which laptops do staff who'd record use? The model name, or the processor and installed RAM from
   **Settings → System → About**.
2. How many are refurbished, donated, or leased under a device-as-a-service contract? Who is the
   supplier, and what spec do they offer at what price?
3. Which Windows edition: Home, Pro, or S-mode? Who installs software: staff, an IT partner, or Intune?
4. What else is open while recording: Teams, a case-management system, browser tabs?
5. When is the next laptop refresh due, and what's the budget per device?

The procurement routes looked at for supplier specs ("DaSS" didn't match a UK framework by that name):
- **DSA (Disabled Students' Allowance) approved suppliers.** SLC's spec matrix has a speech-to-text
  laptop category, minimum i5 10th gen or Ryzen 5 4000, **8 GB**, 500 GB SSD (2021 matrix, the latest
  found). That would put DSA users in Essentials.
- **DaaS (device-as-a-service)** under CCS TePAS 2/3, or the NHS SBS/NOE Tech Devices LINK4 framework,
  whose Lot 4 is refurbished devices. No public spec or price lists were found for either.

## Sources

- Charity Digital Skills Report [2026](https://charitydigitalskills.co.uk/wp-content/uploads/2026/07/Charity-Digital-Skills-Report-2026-1.pdf), [2025](https://charitydigitalskills.co.uk/wp-content/uploads/2025/07/Charity-Digital-Skills-Report-2025.pdf)
- [Charity Digital: finding refurbished tech](https://charitydigital.org.uk/topics/how-charities-can-find-refurbished-tech-10985); [Computers 4 Good listings](https://computers4good.com/refurbished-computers-eshop/refurbished-laptops-for-sale)
- [Statcounter UK Windows versions](https://gs.statcounter.com/os-version-market-share/windows/desktop/united-kingdom)
- [CONTEXT: AI notebooks in Western Europe, Q3 2026](https://www.contextworld.com/blog/-/blogs/ai-notebooks-in-western-europe-40-tops-models-reach-26-in-q3-2026)
- [PCWorld on IDC Copilot+ share](https://www.pcworld.com/article/2816617/microsofts-copilot-gamble-is-a-bustbut-ai-pcs-still-feel-inevitable.html); [Gartner via ecommercenews](https://ecommercenews.com.au/story/ai-pcs-to-make-up-31-of-global-shipments-by-end-of-2025); [Counterpoint](https://counterpointresearch.com/en/reports/ai-advanced-pcs-to-surpass-half-of-global-shipments-in-2026); [Omdia Q3 2026 via StockTitan](https://www.stocktitan.net/news/TTGT/omdia-global-pc-shipments-fall-21-in-3q26-as-pull-forward-demand-rhc1jfolr85u.html)
- [Tom's Hardware: 8 GB is back](https://www.tomshardware.com/laptops/8gb-of-ram-is-back-on-laptops-companies-are-lowering-memory-offerings-to-make-affordable-notebooks-during-component-crisis); [Windows Latest: 8 GB PCs are back](https://www.windowslatest.com/2026/08/18/exclusive-oem-specs-confirm-8gb-ram-windows-11-pcs-are-back-but-you-still-need-16gb-for-ai-features/)
- UK listings: [Lenovo V15 G4](https://www.laptopoutlet.co.uk/lenovo-v15-g4-intel-83a100amuk.html), [ASUS ExpertBook P1403](https://business.currys.co.uk/catalogue/computing/laptops/windows-laptop/asus-expertbook-p1-p1403cv-i582x-14-intel-core-i5-13420h-8-gb-ram-256-gb-ssd/P309343P), [ThinkPad E14 Gen 7](https://www.misco.co.uk/product/Computers/Laptops/Lenovo/Lenovo-ThinkPad-E14-Gen-7---14---Intel-Core-Ultra?prodid=12003968), [HP ProBook 440 G11](https://www.hp.com/gb-en/shop/products/laptops/hp-probook-440-14-inch-g11-notebook-pc-wolf-pro-security-edition-a23fnea-abu), [Dell Pro 14 Essential memory (single-channel 8 GB)](https://www.dell.com/support/manuals/en-us/dell-pro-pc14255-laptop/dell-pro-14-pc14255-owners-manual/memory?guid=guid-e9d2ea83-38a4-431d-803c-96d63c1dbc34&lang=en-us)
- NPU specs: [Intel Core Ultra 5 225U](https://www.intel.com/content/www/us/en/products/sku/241861/intel-core-ultra-5-processor-225u-12m-cache-up-to-4-80-ghz/specifications.html), [Wildcat Lake (ServeTheHome)](https://www.servethehome.com/intel-launches-core-series-3-wildcat-lake-mobile-processors-for-low-cost-laptops/), [AMD Ryzen 5 PRO 230](https://www.amd.com/en/products/processors/laptop/ryzen-pro/200-series/amd-ryzen-5-pro-230.html), [Intel N150 (Notebookcheck)](https://www.notebookcheck.net/Intel-Processor-N150-Processor-Benchmarks-and-Specs.951351.0.html)
- [Microsoft Learn: Phi Silica (Aion Instruct notice)](https://learn.microsoft.com/en-us/windows/ai/apis/phi-silica); [Windows ML execution providers](https://learn.microsoft.com/en-us/windows/ai/new-windows-ml/supported-execution-providers); [S-mode FAQ](https://support.microsoft.com/en-us/windows/windows-10-and-windows-11-in-s-mode-faq-851057d6-1ee9-b9e5-c30b-93baebeebc85)
- Procurement: [SLC DSA spec matrix 2021](https://www.practitioners.slc.co.uk/media/1915/slc-spec-matrix-2021.pdf), [CCS/GCA RM6098 TePAS 2](https://www.gca.gov.uk/agreements/RM6098), [NHS Tech Devices LINK4](https://www.lpp.nhs.uk/categories/digital-technology-solutions/tech-devices-link4/)
