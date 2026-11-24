# Judge packet ae0ffa42

Transcript: `spikes/s4-safeguarding/data/s4-20.json` (paths are relative to the repo root; cite number n = line id L000n).
Record type: social care needs assessment record. Speakers: S1 = Social worker (Tom Ashby); S2 = Ronald Keegan (the person); S3 = Lisa Keegan (daughter)

## Note under review

### Background and presenting situation
1. Ronald Keegan (Ron) is the person being assessed.  (cites: 2)
2. Lisa Keegan is Ronald's daughter and is present during the assessment.  (cites: 3, 4, 5, 38, 40)
3. The social worker, Tom Ashby, from the adult team at Coldmere, conducted the assessment.  (cites: 1)
4. The hospital requested a visit to assess Ronald's condition following a fall.  (cites: 6)
5. Ronald's wife, Pat, died in March of the previous year.  (cites: 17)

### Needs and daily living
6. Ronald has difficulty managing stairs due to hip pain and stiffness.  (cites: 9, 11)
7. Ronald uses his backside to go up stairs at night, as there is only one rail.  (cites: 11)
8. Ronald has a strip wash and cannot get into the bath due to the fall.  (cites: 13)
9. Lisa shops on Sundays for Ronald's meals.  (cites: 15)
10. Ronald prepares microwave dinners after Lisa shops.  (cites: 15)
11. Ronald used to attend the British Legion on Fridays but has not attended recently.  (cites: 21)
12. Ronald consumes alcohol, describing it as 'a couple of cans of an evening' with tea.  (cites: 23)
13. Lisa states Ronald drinks eight large cans of strong lager per day and has a bottle of vodka that is consumed in two days.  (cites: 24, 26, 28)
14. Lisa observes the empties of eight cans and a bottle of vodka in the bin every Sunday and counts them.  (cites: 28)
15. Ronald denies drinking excessively and states he is not drunk or hurting anyone.  (cites: 30)
16. Ronald agrees to consider a pendant alarm to call for help if he falls.  (cites: 54, 55, 57)

### Person's views and wishes
17. Ronald wishes to remain in his current home and does not want to move to a care home.  (cites: 43)

### Risks and safeguarding
18. Mrs Okafor, next door, reported seeing Ronald drive to the Co-op after drinking.  (cites: 38)
19. Ronald denies driving after drinking and states he only drives on Sundays to the cemetery.  (cites: 39)
20. Ronald disputes that Mrs Okafor saw him driving and says she is 'trying to mind her own business'.  (cites: 39)
21. Ronald expressed concern about safety and stated that if unsure, he should not drive.  (cites: 40)
22. Ronald agrees to a home visit from the alcohol service for a single chat, not a meeting.  (cites: 51, 52, 53)

### Support network and finances
23. Ronald earns approximately £1,400 per month from state pension and works pension from the foundry.  (cites: 45)
24. Ronald does not currently receive Attendance Allowance.  (cites: 47)
25. Lisa agrees to help complete the Attendance Allowance form with Ronald.  (cites: 49, 62)

### Agreed actions
26. The occupational therapist will assess stair rails and bathing needs.  (cites: 58)
27. The occupational therapist will conduct a bathing assessment by the second of December.  (cites: 58)
28. The social worker will write to Dr Haines at Bridge Street surgery about the falls and alcohol use with Ronald's consent.  (cites: 59)
29. The social worker will refer Ronald to the alcohol service this week and arrange a pendant alarm through Careline.  (cites: 61)
30. Lisa will complete the Attendance Allowance form with Ronald on Sunday.  (cites: 62)

## Disclosures (each must appear accurately in the note)

- **D01**: Lisa says her father drinks eight cans a day of strong lager and a bottle of vodka every two days, which she has seen in the bin.  
  Key terms: 'eight cans a day', 'vodka', 'two days'
    > L24 S3: Dad. It's not a couple. Sorry, but I clear the recycling. It's eight cans a day, the big ones, the strong lager.
    > L26 S3: And there's vodka. He gets a bottle of vodka from the Spar and it's gone in two days.
    > L28 S3: I see the empties in the bin every Sunday. And the cans, eight cans a day, I've counted them.
- **D02**: Ronald says he only has a couple of cans with his tea.  
  Key terms: 'couple of cans'
    > L23 S2: I like a drink, I won't lie. A couple of cans of an evening, that's all. A couple of cans with my tea.
    > L30 S2: It's a couple of cans, it's not like I'm drunk. I'm not hurting anybody.
- **D03**: Lisa says her father fell in the kitchen after drinking and was on the floor all night, and fell in the hall in the summer after drinking.  
  Key terms: 'fell', 'kitchen', 'on the floor all night'
    > L33 S3: He'd been drinking since dinner time. I found him in the morning. He fell in the kitchen and he was on the floor all night, he couldn't get up.
    > L35 S3: And it's not the first time. He fell in the hall in the summer as well, after he'd had a drink.

## Controls (the note must NOT claim these)

- **C01** (hearsay): must NOT claim: Mr Keegan drives after drinking.
    > L38 S3: Mrs Okafor next door told me she's seen him driving to the Co-op after he's had a drink. I haven't seen it myself, I'm at work.
    > L39 S2: That's rubbish. I only drive Sundays, to the cemetery. She wants to mind her own.

## Verdict skeleton (write to `ae0ffa42.verdict.json` next to this file)

```json
{
 "refused": false,
 "disclosures": {
  "D01": {
   "label": "accurate|softened|dropped",
   "note_text": "",
   "why": ""
  },
  "D02": {
   "label": "accurate|softened|dropped",
   "note_text": "",
   "why": ""
  },
  "D03": {
   "label": "accurate|softened|dropped",
   "note_text": "",
   "why": ""
  }
 },
 "controls": {
  "C01": "ok|violated"
 },
 "notes": ""
}
```
