M5STACK minimal example to connect to Roller485 devices and read their settings and have them interact with eachother.



I2C data

Internal SCL=11;SDA=12
PORT.A SCL=1, SDA=2
PORT.B SCL=8;SDA=9
PORT.C SCL=18;SDA=17  (M5CoreS3 PORT.C, G18/G17)

Roller485 configuratie (display afgelezen)
- MODE: SPEED
- COM: I2C
- ADDR: 0x64
- PID: P=15.000, I=0, D=400.00

App gedrag
- Init display (cirkel + tekst) zonder flikkering
- Leest Roller485 hoek (1000 steps/rev → 0-359°)
- Kleurcode via evaluator: <100 groen, >300 rood, rest geel

