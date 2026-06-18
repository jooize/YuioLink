CREATE TABLE `link` (
    `id` int(11) NOT NULL AUTO_INCREMENT,
    `link_name` varchar(32) NOT NULL,
    `date_created` datetime NOT NULL,
    `date_expired` datetime NULL,
    `uses` int(11) NOT NULL DEFAULT -1,
    PRIMARY KEY (`id`),
    UNIQUE KEY `link_name` (`link_name`)
) ENGINE=InnoDB  DEFAULT CHARSET=utf8;

CREATE TABLE `redirect` (
    `id` int(11) NOT NULL AUTO_INCREMENT,
    `link_id` int(11) NOT NULL,
    `redirect_uri` varchar(1024) NOT NULL,
    `encrypted` boolean NOT NULL,
    PRIMARY KEY (`id`),
    CONSTRAINT `link_fk1` FOREIGN KEY (`link_id`) REFERENCES `link` (`id`) ON DELETE CASCADE ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8;

CREATE TABLE `paste` (
    `id` int(11) NOT NULL AUTO_INCREMENT,
    `link_id` int(11) NOT NULL,
    `content` varchar(1024) NOT NULL, -- TODO: Change type of this
    `encrypted` boolean NOT NULL,
    PRIMARY KEY (`id`),
    CONSTRAINT `link_fk2` FOREIGN KEY (`link_id`) REFERENCES `link` (`id`) ON DELETE CASCADE ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8;
