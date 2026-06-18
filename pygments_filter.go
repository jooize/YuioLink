package main

import (
	"bytes"
	log "github.com/Sirupsen/logrus"
	"github.com/flosch/pongo2"
	"os/exec"
	"strings"
)

func init() {
	pongo2.RegisterFilter("pygments", pygmentsFilter)
}

func pygmentsFilter(in *pongo2.Value, param *pongo2.Value) (*pongo2.Value, *pongo2.Error) {
	code := in.String()
	contentType := param.String()

	var out bytes.Buffer
	var errbuf bytes.Buffer
	command := exec.Command("pygmentize", "-l"+contentType, "-fhtml", "-O", "noclasses=true")
	command.Stdin = strings.NewReader(code)
	command.Stderr = &errbuf
	command.Stdout = &out

	if err := command.Run(); err != nil {
		log.Errorf("%s, %s", err.Error(), errbuf.String())
		return pongo2.AsSafeValue(code), nil
	}

	return pongo2.AsSafeValue(out.String()), nil
}
