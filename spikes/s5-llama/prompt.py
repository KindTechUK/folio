"""Print the synthetic prompt S5's generation timings used (730 tokens with Qwen3's chat template):
24 lines of a housing-visit transcript and an instruction to list its facts."""
lines = []
names = ["Sam", "Priya"]
facts = ["the boiler was serviced in March", "the tenant reported damp in the back bedroom",
         "rent is paid on the first of the month", "the landlord agreed to inspect the window seals",
         "a follow-up visit is booked for next Thursday", "the tenant has two children under five",
         "the council letter arrived on the twelfth", "the deposit is held in a protection scheme"]
for i in range(24):
    lines.append(f"[{i*15//60:02d}:{i*15%60:02d}] {names[i%2]}: I can confirm that {facts[i%len(facts)]}, "
                 "and we discussed what happens next.")
print("Write a short note of the key facts from this transcript excerpt, as bullet points.\n\n" + "\n".join(lines))
