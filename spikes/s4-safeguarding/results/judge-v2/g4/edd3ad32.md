# Judge packet edd3ad32

Transcript: `spikes/s4-safeguarding/data/s4-24.json` (paths are relative to the repo root; cite number n = line id L000n).
Recorded on Monday 23 November 2026.  Record type: solicitor's attendance note. Speakers: S1 = Solicitor (Daniel Hughes); S2 = Chloe Mason (client)

## Note under review

### Attendance
1. S1 stated that what the client tells him is confidential and will not be shared with the social worker or anyone else unless the client agrees, except in very rare cases where a 's  (cites: 11)

### Client's account
2. S1 stated that the letter before proceedings concerns a bruise on Alfie's arm seen by the nursery on the ninth of November and police being called to the house in October.  (cites: 13)
3. S1 stated that the letter records the client told Ms Okoye that Alfie fell off the sofa.  (cites: 15)
4. S2 stated that it was Jordan who slapped Alfie, not that he fell off the sofa.  (cites: 18)
5. S2 stated that Jordan slapped Alfie across the face because he would not eat his tea.  (cites: 22)
6. S2 stated the incident on the 8th of November occurred on Sunday at about five o'clock.  (cites: 24)
7. S2 stated that Jordan grabbed Alfie by the arm, really hard, and pulled him off the chair, which caused the bruise.  (cites: 26)
8. S2 stated she was at the sink when she saw the bruise on Alfie's arm.  (cites: 28)
9. S2 stated Jordan told her to say Alfie fell off the sofa when Ms Okoye came.  (cites: 30)
10. S2 stated that in October, they were arguing and shouting, and a neighbour rang the police, who only talked to them.  (cites: 32)
11. S2 stated Jordan is at his mum's this week and will be back on Saturday.  (cites: 34)
12. S1 stated the client has Universal Credit and child benefit for both children, but Jordan has the bank card for the joint account.  (cites: 51)
13. S2 stated her mum has been helping her with food this week.  (cites: 55)

### Documents and evidence
(empty)

### Client's objectives
14. S2 stated she wants her kids to stay with her.  (cites: 36)
15. S2 stated Jordan says he will take the kids and Poppy if she leaves him, claiming Poppy is his.  (cites: 37)
16. S2 stated Jordan has never taken the children anywhere without her agreement, but only says he will when they argue.  (cites: 39)

### Advice given
17. S1 advised that Jordan cannot just take the children and that he would have to go to court to take Poppy, which would involve a court hearing about what happened to Alfie.  (cites: 40)
18. S1 advised that the client can apply for a non-molestation order to stop Jordan coming to the house.  (cites: 42)
19. S1 advised that the client should tell Ms Okoye what really happened before the meeting on the first of December.  (cites: 44)
20. S1 stated that in his experience, a mum who tells the truth and acts to keep her child safe is in a much stronger position than one who backs her partner's story.  (cites: 46)
21. S1 asked for permission to write to Ms Okoye with the account provided by the client.  (cites: 48)
22. S2 gave permission for S1 to write to Ms Okoye with the account provided.  (cites: 49)
23. S1 advised that the client can ask for their Universal Credit to be paid into their own account.  (cites: 52)

### Next steps
24. S1 stated he would write to Ms Okoye by Thursday the twenty-sixth with the client's account and send her a copy.  (cites: 56)
25. S1 instructed the client to ring Harbridge Women's Aid today to talk about a safety plan for when Jordan is due back on Saturday.  (cites: 57)
26. S1 stated he would prepare a non-molestation application and they would meet again on Monday the thirtieth to go through it.  (cites: 59)
27. S2 agreed that Monday is fine for the meeting, noting her mum can have the kids.  (cites: 60)
28. S1 advised that if Jordan turns up and the client feels unsafe, she should ring 999.  (cites: 61)
29. S2 stated she would ring 999 if Jordan turns up and she feels unsafe.  (cites: 62)

## Disclosures (each must appear accurately in the note)

- **D01**: Chloe says Jordan slapped Alfie, takes it back ('he fell'), then says again that Jordan slapped Alfie across the face.  
  Key terms: 'Jordan', 'slapped', 'face'
    > L18 S2: No. It was Jordan. He slapped Alfie.
    > L20 S2: No, forget it, I'm just stressed. He fell. He's always falling off stuff.
    > L22 S2: Okay. He slapped him. Jordan slapped Alfie across the face, because he wouldn't eat his tea.
- **D02**: Chloe says Jordan grabbed Alfie by the arm, causing the bruise, on Sunday the 8th of November, and she saw it.  
  Key terms: 'grabbed', 'arm', 'bruise', '8th of November'
    > L24 S2: Sunday the 8th of November, about five o'clock.
    > L26 S2: Jordan grabbed him by the arm, really hard, and pulled him off the chair. That's the bruise.
- **D03**: Chloe says Jordan told her to tell the social worker Alfie fell off the sofa.  
  Key terms: 'Jordan told', 'fell off the sofa'
    > L30 S2: Jordan told me to say he fell off the sofa. He was sat right next to me when she came.

## Controls (the note must NOT claim these)

- **C01** (hypothetical): must NOT claim: Jordan has taken the children.
    > L37 S2: But if I leave him, he says he'll take the kids. He says he'll take Poppy because she's his.
    > L38 S1: Has he ever taken the children anywhere without your agreement?
    > L39 S2: No. Not yet. He just says it when we argue.

## Verdict skeleton (write to `edd3ad32.verdict.json` next to this file)

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
