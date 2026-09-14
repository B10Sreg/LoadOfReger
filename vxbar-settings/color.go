package main

import (
	"fmt"
	"math"

	"github.com/diamondburned/gotk4/pkg/gdk/v4"
)

// hexToRGBA понимает те же формы, что vxbar/src/config.rs::parse_color:
// #rgb, #rrggbb, #rrggbbaa. Всё остальное даёт мадженту — ровно как в баре,
// чтобы опечатка в конфиге выглядела одинаково и в панели, и в настройках.
func hexToRGBA(s string) *gdk.RGBA {
	var r, g, b, a uint8 = 255, 0, 255, 255
	switch len(s) {
	case 4:
		var rr, gg, bb uint8
		if _, err := fmt.Sscanf(s, "#%1x%1x%1x", &rr, &gg, &bb); err == nil {
			r, g, b, a = rr*17, gg*17, bb*17, 255
		}
	case 7:
		var rr, gg, bb uint8
		if _, err := fmt.Sscanf(s, "#%2x%2x%2x", &rr, &gg, &bb); err == nil {
			r, g, b, a = rr, gg, bb, 255
		}
	case 9:
		var rr, gg, bb, aa uint8
		if _, err := fmt.Sscanf(s, "#%2x%2x%2x%2x", &rr, &gg, &bb, &aa); err == nil {
			r, g, b, a = rr, gg, bb, aa
		}
	}
	c := gdk.NewRGBA(
		float32(r)/255, float32(g)/255, float32(b)/255, float32(a)/255,
	)
	return &c
}

// rgbaToHex опускает альфу, когда она полная: чистый #rrggbb читается в
// конфиге приятнее, а бар трактует обе формы одинаково.
func rgbaToHex(c *gdk.RGBA) string {
	q := func(v float32) uint8 {
		return uint8(math.Round(float64(clamp01(v)) * 255))
	}
	r, g, b, a := q(c.Red()), q(c.Green()), q(c.Blue()), q(c.Alpha())
	if a == 255 {
		return fmt.Sprintf("#%02x%02x%02x", r, g, b)
	}
	return fmt.Sprintf("#%02x%02x%02x%02x", r, g, b, a)
}

func clamp01(v float32) float32 {
	if v < 0 {
		return 0
	}
	if v > 1 {
		return 1
	}
	return v
}
